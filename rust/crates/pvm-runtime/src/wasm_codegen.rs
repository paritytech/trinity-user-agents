/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    MAX_GUEST_HEAP_BYTES, MAX_GUEST_RW_DATA_BYTES, MAX_GUEST_STACK_BYTES, MAX_PROGRAM_BYTES,
};
use anyhow::{anyhow, bail, Context, Result};
use polkavm::program::{Instruction as PvmInstruction, ParsedInstruction, RawReg};
use polkavm::{MemoryMapBuilder, ProgramBlob, Reg, RETURN_TO_HOST};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, CustomSection, DataSection, ElementSection, Elements,
    ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType,
    Instruction as W, MemArg, MemorySection, MemoryType, Module, RefType, TableSection, TableType,
    TypeSection, ValType,
};

const PAGE_SIZE: u32 = 65_536;
const STATUS_FINISHED: i32 = -1;
const STATUS_ECALL: i32 = -2;
const STATUS_TRAP: i32 = -3;
const STATUS_OUT_OF_GAS: i32 = -4;
const REGISTER_COUNT: u32 = 13;

const TYPE_BLOCK: u32 = 0;
const TYPE_BINARY_I64: u32 = 1;
const TYPE_BEGIN: u32 = 2;
const TYPE_SET_GAS: u32 = 3;
const TYPE_UNARY_I64: u32 = 4;

const GLOBAL_PC: u32 = REGISTER_COUNT;
const GLOBAL_GAS: u32 = GLOBAL_PC + 1;
const GLOBAL_HEAP_SIZE: u32 = GLOBAL_GAS + 1;
const GLOBAL_ECALL: u32 = GLOBAL_HEAP_SIZE + 1;
const GLOBAL_TRAP_PC: u32 = GLOBAL_ECALL + 1;

const LOCAL_ADDR: u32 = 0;
const LOCAL_PHYS: u32 = 1;
const LOCAL_I64_0: u32 = 2;
const LOCAL_I64_1: u32 = 3;

#[derive(Clone, Copy)]
struct Layout {
    ro_address: u32,
    ro_size: u32,
    ro_phys: u32,
    rw_address: u32,
    rw_size: u32,
    rw_phys: u32,
    heap_base: u32,
    heap_limit: u32,
    stack_low: u32,
    stack_high: u32,
    stack_phys: u32,
    ro_pages: u64,
    rw_pages: u64,
    rw_max_pages: u64,
    stack_pages: u64,
}

#[derive(Clone, Copy)]
enum LoadKind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
}

#[derive(Clone, Copy)]
enum StoreKind {
    U8,
    U16,
    U32,
    U64,
}

pub fn translate(program: &[u8]) -> Result<Vec<u8>> {
    if program.is_empty() || program.len() > MAX_PROGRAM_BYTES {
        bail!("guest program exceeds browser limit");
    }
    let blob =
        ProgramBlob::parse(program.into()).context("parse PolkaVM program for Wasm translation")?;
    blob.validate_code_with_isa(blob.isa())
        .map_err(|pc| anyhow!("invalid PolkaVM instruction at {pc}"))?;
    if blob.stack_size() > MAX_GUEST_STACK_BYTES {
        bail!("guest stack exceeds browser limit");
    }
    if blob.rw_data_size() > MAX_GUEST_RW_DATA_BYTES {
        bail!("guest read-write data exceeds browser limit");
    }

    let instructions: Vec<_> = blob.instructions().collect();
    if instructions.is_empty() {
        bail!("PolkaVM program contains no instructions");
    }
    let metered_targets = collect_metered_targets(&instructions);

    let layout = build_layout(&blob)?;
    let targets = collect_block_targets(&blob, &instructions)?;
    let (blocks, block_by_pc) = build_blocks(&instructions, &targets)?;
    let jump_targets: Vec<_> = blob.jump_table().into_iter().collect();

    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    types
        .ty()
        .function([ValType::I64, ValType::I64], [ValType::I64]);
    types
        .ty()
        .function([ValType::I32, ValType::I64], [ValType::I32]);
    types.ty().function([ValType::I64], []);
    types.ty().function([ValType::I64], [ValType::I64]);
    module.section(&types);

    let helper_count = 3u32;
    let block_base = helper_count;
    let resolver_base = block_base + blocks.len() as u32;
    let dispatcher_index = resolver_base + jump_targets.len() as u32;
    let begin_index = dispatcher_index + 1;
    let set_gas_index = begin_index + 1;

    let mut functions = FunctionSection::new();
    functions.function(TYPE_BINARY_I64);
    functions.function(TYPE_UNARY_I64);
    functions.function(TYPE_UNARY_I64);
    for _ in &blocks {
        functions.function(TYPE_BLOCK);
    }
    for _ in &jump_targets {
        functions.function(TYPE_BLOCK);
    }
    functions.function(TYPE_BLOCK);
    functions.function(TYPE_BEGIN);
    functions.function(TYPE_SET_GAS);
    module.section(&functions);

    let mut table = TableSection::new();
    table.table(TableType {
        element_type: RefType::FUNCREF,
        minimum: (blocks.len() + jump_targets.len()) as u64,
        maximum: Some((blocks.len() + jump_targets.len()) as u64),
        table64: false,
        shared: false,
    });
    module.section(&table);

    let mut memory = MemorySection::new();
    for (minimum, maximum) in [
        (layout.ro_pages, layout.ro_pages),
        (layout.rw_pages, layout.rw_max_pages),
        (layout.stack_pages, layout.stack_pages),
    ] {
        memory.memory(MemoryType {
            minimum,
            maximum: Some(maximum),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
    }
    module.section(&memory);

    let mut globals = GlobalSection::new();
    for index in 0..REGISTER_COUNT {
        globals.global(
            GlobalType {
                val_type: ValType::I64,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i64_const(if index == Reg::SP.to_u32() {
                layout.stack_high as i64
            } else {
                0
            }),
        );
    }
    for _ in 0..5 {
        let index = globals.len();
        if index == GLOBAL_GAS || index == GLOBAL_HEAP_SIZE {
            globals.global(
                GlobalType {
                    val_type: ValType::I64,
                    mutable: true,
                    shared: false,
                },
                &ConstExpr::i64_const(0),
            );
        } else {
            globals.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: true,
                    shared: false,
                },
                &ConstExpr::i32_const(0),
            );
        }
    }
    module.section(&globals);

    let mut exports = ExportSection::new();
    exports.export("memory_ro", ExportKind::Memory, 0);
    exports.export("memory_rw", ExportKind::Memory, 1);
    exports.export("memory_stack", ExportKind::Memory, 2);
    exports.export("pvm_begin", ExportKind::Func, begin_index);
    exports.export("pvm_resume", ExportKind::Func, dispatcher_index);
    exports.export("pvm_set_gas", ExportKind::Func, set_gas_index);
    for reg in Reg::ALL {
        exports.export(reg.name_non_abi(), ExportKind::Global, reg.to_u32());
    }
    exports.export("pc", ExportKind::Global, GLOBAL_PC);
    exports.export("gas", ExportKind::Global, GLOBAL_GAS);
    exports.export("heap_size", ExportKind::Global, GLOBAL_HEAP_SIZE);
    exports.export("ecall", ExportKind::Global, GLOBAL_ECALL);
    exports.export("trap_pc", ExportKind::Global, GLOBAL_TRAP_PC);
    module.section(&exports);

    let table_functions: Vec<u32> =
        (block_base..resolver_base + jump_targets.len() as u32).collect();
    let mut elements = ElementSection::new();
    elements.active(
        Some(0),
        &ConstExpr::i32_const(0),
        Elements::Functions(Cow::Owned(table_functions)),
    );
    module.section(&elements);

    let mut code = CodeSection::new();
    code.function(&emit_mulhu());
    code.function(&emit_bswap32());
    code.function(&emit_bswap64());

    let context = EmitContext {
        layout,
        block_by_pc: &block_by_pc,
        jump_table_len: jump_targets.len() as u32,
        resolver_table_base: blocks.len() as u32,
        block_function_base: block_base,
        mulhu_function: 0,
        bswap32_function: 1,
        metered_targets: &metered_targets,
        bswap64_function: 2,
        is_64_bit: blob.is_64_bit(),
    };
    for block in &blocks {
        code.function(&emit_block(&context, block)?);
    }
    for target in &jump_targets {
        let mut function = Function::new([]);
        function.instruction(&W::I32Const(
            block_by_pc
                .get(&target.0)
                .copied()
                .map(|index| index as i32)
                .unwrap_or(STATUS_TRAP),
        ));
        function.instruction(&W::End);
        code.function(&function);
    }
    code.function(&emit_dispatcher());
    code.function(&emit_begin());
    code.function(&emit_set_gas());
    module.section(&code);

    let mut data = DataSection::new();
    if !blob.ro_data().is_empty() {
        data.active(
            0,
            &ConstExpr::i32_const(layout.ro_phys as i32),
            blob.ro_data().iter().copied(),
        );
    }
    if !blob.rw_data().is_empty() {
        data.active(
            1,
            &ConstExpr::i32_const(layout.rw_phys as i32),
            blob.rw_data().iter().copied(),
        );
    }
    module.section(&data);

    let metadata = encode_metadata(&blob, &block_by_pc, layout)?;
    module.section(&CustomSection {
        name: Cow::Borrowed("epoca.pvm.meta"),
        data: Cow::Owned(metadata),
    });
    Ok(module.finish())
}

fn align(value: u32, alignment: u32) -> Result<u32> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or_else(|| anyhow!("translated memory layout overflow"))
}

fn build_layout(blob: &ProgramBlob) -> Result<Layout> {
    let map = MemoryMapBuilder::new(PAGE_SIZE)
        .ro_data_size(blob.ro_data_size())
        .rw_data_size(blob.rw_data_size())
        .stack_size(blob.stack_size())
        .build()
        .map_err(|error| anyhow!(error))?;
    let heap_limit = MAX_GUEST_HEAP_BYTES.min(map.max_heap_size());
    let rw_max_bytes = map
        .heap_base()
        .wrapping_sub(map.rw_data_address())
        .checked_add(heap_limit)
        .ok_or_else(|| anyhow!("translated heap layout overflow"))?;
    let pages = |bytes: u32| align(bytes, PAGE_SIZE).map(|bytes| u64::from(bytes / PAGE_SIZE));
    Ok(Layout {
        ro_address: map.ro_data_address(),
        ro_size: map.ro_data_size(),
        ro_phys: 0,
        rw_address: map.rw_data_address(),
        rw_size: map.rw_data_size(),
        rw_phys: 0,
        heap_base: map.heap_base(),
        heap_limit,
        stack_low: map.stack_address_low(),
        stack_high: map.stack_address_high(),
        stack_phys: 0,
        ro_pages: pages(map.ro_data_size())?,
        rw_pages: pages(map.rw_data_size())?,
        rw_max_pages: pages(rw_max_bytes)?,
        stack_pages: pages(map.stack_size())?,
    })
}

fn is_terminator(kind: PvmInstruction) -> bool {
    matches!(
        kind,
        PvmInstruction::trap
            | PvmInstruction::jump(..)
            | PvmInstruction::jump_indirect(..)
            | PvmInstruction::load_imm_and_jump(..)
            | PvmInstruction::load_imm_and_jump_indirect(..)
            | PvmInstruction::branch_eq(..)
            | PvmInstruction::branch_not_eq(..)
            | PvmInstruction::branch_less_unsigned(..)
            | PvmInstruction::branch_less_signed(..)
            | PvmInstruction::branch_greater_or_equal_unsigned(..)
            | PvmInstruction::branch_greater_or_equal_signed(..)
            | PvmInstruction::branch_eq_imm(..)
            | PvmInstruction::branch_not_eq_imm(..)
            | PvmInstruction::branch_less_unsigned_imm(..)
            | PvmInstruction::branch_less_signed_imm(..)
            | PvmInstruction::branch_greater_or_equal_unsigned_imm(..)
            | PvmInstruction::branch_greater_or_equal_signed_imm(..)
            | PvmInstruction::branch_less_or_equal_signed_imm(..)
            | PvmInstruction::branch_less_or_equal_unsigned_imm(..)
            | PvmInstruction::branch_greater_signed_imm(..)
            | PvmInstruction::branch_greater_unsigned_imm(..)
            | PvmInstruction::ecalli(..)
    )
}

fn direct_target(kind: PvmInstruction) -> Option<u32> {
    match kind {
        PvmInstruction::jump(target) | PvmInstruction::load_imm_and_jump(_, _, target) => {
            Some(target)
        }
        PvmInstruction::branch_eq(_, _, target)
        | PvmInstruction::branch_not_eq(_, _, target)
        | PvmInstruction::branch_less_unsigned(_, _, target)
        | PvmInstruction::branch_less_signed(_, _, target)
        | PvmInstruction::branch_greater_or_equal_unsigned(_, _, target)
        | PvmInstruction::branch_greater_or_equal_signed(_, _, target)
        | PvmInstruction::branch_eq_imm(_, _, target)
        | PvmInstruction::branch_not_eq_imm(_, _, target)
        | PvmInstruction::branch_less_unsigned_imm(_, _, target)
        | PvmInstruction::branch_less_signed_imm(_, _, target)
        | PvmInstruction::branch_greater_or_equal_unsigned_imm(_, _, target)
        | PvmInstruction::branch_greater_or_equal_signed_imm(_, _, target)
        | PvmInstruction::branch_less_or_equal_signed_imm(_, _, target)
        | PvmInstruction::branch_less_or_equal_unsigned_imm(_, _, target)
        | PvmInstruction::branch_greater_signed_imm(_, _, target)
        | PvmInstruction::branch_greater_unsigned_imm(_, _, target) => Some(target),
        _ => None,
    }
}

fn collect_block_targets(
    blob: &ProgramBlob,
    instructions: &[ParsedInstruction],
) -> Result<BTreeSet<u32>> {
    let valid: BTreeSet<_> = instructions
        .iter()
        .map(|instruction| instruction.offset.0)
        .collect();
    let mut targets = BTreeSet::from([instructions[0].offset.0]);
    for export in blob.exports() {
        targets.insert(export.program_counter().0);
    }
    for target in blob.jump_table() {
        targets.insert(target.0);
    }
    for instruction in instructions {
        if let Some(target) = direct_target(instruction.kind) {
            targets.insert(target);
        }
        if is_terminator(instruction.kind) && valid.contains(&instruction.next_offset.0) {
            targets.insert(instruction.next_offset.0);
        }
    }
    if let Some(invalid) = targets.iter().find(|target| !valid.contains(target)) {
        bail!("PolkaVM control-flow target {invalid} is not an instruction");
    }
    Ok(targets)
}

fn build_blocks(
    instructions: &[ParsedInstruction],
    targets: &BTreeSet<u32>,
) -> Result<(Vec<Vec<ParsedInstruction>>, BTreeMap<u32, u32>)> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    for instruction in instructions {
        if targets.contains(&instruction.offset.0) && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
        current.push(*instruction);
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    let mut block_by_pc = BTreeMap::new();
    for (index, block) in blocks.iter().enumerate() {
        block_by_pc.insert(block[0].offset.0, index as u32);
    }
    Ok((blocks, block_by_pc))
}

fn collect_metered_targets(instructions: &[ParsedInstruction]) -> BTreeSet<u32> {
    let mut targets = BTreeSet::new();
    for instruction in instructions {
        if let Some(target) = direct_target(instruction.kind) {
            if target <= instruction.offset.0 {
                targets.insert(target);
            }
        }
    }
    targets
}

struct EmitContext<'a> {
    layout: Layout,
    block_by_pc: &'a BTreeMap<u32, u32>,
    jump_table_len: u32,
    resolver_table_base: u32,
    block_function_base: u32,
    mulhu_function: u32,
    bswap32_function: u32,
    metered_targets: &'a BTreeSet<u32>,
    bswap64_function: u32,
    is_64_bit: bool,
}

fn reg_index(reg: RawReg) -> u32 {
    reg.get().to_u32()
}

fn memarg(bytes: u32, memory_index: u32) -> MemArg {
    MemArg {
        offset: 0,
        align: bytes.trailing_zeros(),
        memory_index,
    }
}

fn emit_mulhu() -> Function {
    let mut f = Function::new([(4, ValType::I64)]);
    // Hacker's Delight 32-bit limb multiplication.
    f.instruction(&W::LocalGet(0));
    f.instruction(&W::I64Const(0xffff_ffff));
    f.instruction(&W::I64And);
    f.instruction(&W::LocalSet(2));
    f.instruction(&W::LocalGet(1));
    f.instruction(&W::I64Const(0xffff_ffff));
    f.instruction(&W::I64And);
    f.instruction(&W::LocalSet(3));
    f.instruction(&W::LocalGet(2));
    f.instruction(&W::LocalGet(3));
    f.instruction(&W::I64Mul);
    f.instruction(&W::I64Const(32));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::LocalGet(0));
    f.instruction(&W::I64Const(32));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::LocalGet(3));
    f.instruction(&W::I64Mul);
    f.instruction(&W::I64Add);
    f.instruction(&W::LocalTee(4));
    f.instruction(&W::I64Const(32));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::LocalSet(5));
    f.instruction(&W::LocalGet(4));
    f.instruction(&W::I64Const(0xffff_ffff));
    f.instruction(&W::I64And);
    f.instruction(&W::LocalGet(2));
    f.instruction(&W::LocalGet(1));
    f.instruction(&W::I64Const(32));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::I64Mul);
    f.instruction(&W::I64Add);
    f.instruction(&W::I64Const(32));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::LocalGet(5));
    f.instruction(&W::I64Add);
    f.instruction(&W::LocalGet(0));
    f.instruction(&W::I64Const(32));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::LocalGet(1));
    f.instruction(&W::I64Const(32));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::I64Mul);
    f.instruction(&W::I64Add);
    f.instruction(&W::End);
    f
}

fn emit_bswap32() -> Function {
    let mut f = Function::new([]);
    for (mask, shift, left) in [
        (0x0000_00ffu32, 24, true),
        (0x0000_ff00u32, 8, true),
        (0x00ff_0000u32, 8, false),
        (0xff00_0000u32, 24, false),
    ] {
        f.instruction(&W::LocalGet(0));
        f.instruction(&W::I32WrapI64);
        f.instruction(&W::I32Const(mask as i32));
        f.instruction(&W::I32And);
        f.instruction(&W::I32Const(shift));
        f.instruction(if left { &W::I32Shl } else { &W::I32ShrU });
        if mask != 0x0000_00ff {
            f.instruction(&W::I32Or);
        }
    }
    f.instruction(&W::I64ExtendI32U);
    f.instruction(&W::End);
    f
}

fn emit_bswap64() -> Function {
    let mut f = Function::new([]);
    for (mask, left, right) in [
        (0x00ff_00ff_00ff_00ffu64, 8, 8),
        (0x0000_ffff_0000_ffffu64, 16, 16),
        (0x0000_0000_ffff_ffffu64, 32, 32),
    ] {
        f.instruction(&W::LocalGet(0));
        f.instruction(&W::I64Const(mask as i64));
        f.instruction(&W::I64And);
        f.instruction(&W::I64Const(left));
        f.instruction(&W::I64Shl);
        f.instruction(&W::LocalGet(0));
        f.instruction(&W::I64Const(!mask as i64));
        f.instruction(&W::I64And);
        f.instruction(&W::I64Const(right));
        f.instruction(&W::I64ShrU);
        f.instruction(&W::I64Or);
        if left != 32 {
            f.instruction(&W::LocalSet(0));
        }
    }
    f.instruction(&W::End);
    f
}

fn emit_dispatcher() -> Function {
    let mut f = Function::new([]);
    f.instruction(&W::GlobalGet(GLOBAL_PC));
    f.instruction(&W::ReturnCallIndirect {
        type_index: TYPE_BLOCK,
        table_index: 0,
    });
    f.instruction(&W::End);
    f
}

fn emit_begin() -> Function {
    let mut f = Function::new([]);
    f.instruction(&W::I64Const(RETURN_TO_HOST as i64));
    f.instruction(&W::GlobalSet(Reg::RA.to_u32()));
    f.instruction(&W::LocalGet(0));
    f.instruction(&W::GlobalSet(GLOBAL_PC));
    f.instruction(&W::LocalGet(1));
    f.instruction(&W::GlobalSet(GLOBAL_GAS));
    f.instruction(&W::LocalGet(0));
    f.instruction(&W::ReturnCallIndirect {
        type_index: TYPE_BLOCK,
        table_index: 0,
    });
    f.instruction(&W::End);
    f
}

fn emit_set_gas() -> Function {
    let mut f = Function::new([]);
    f.instruction(&W::LocalGet(0));
    f.instruction(&W::GlobalSet(GLOBAL_GAS));
    f.instruction(&W::End);
    f
}

fn encode_metadata(
    blob: &ProgramBlob,
    block_by_pc: &BTreeMap<u32, u32>,
    layout: Layout,
) -> Result<Vec<u8>> {
    let mut bytes = b"EPM2".to_vec();
    bytes.extend_from_slice(&u32::from(blob.is_64_bit()).to_le_bytes());
    for value in [
        layout.ro_address,
        layout.ro_size,
        layout.ro_phys,
        layout.rw_address,
        layout.rw_size,
        layout.rw_phys,
        layout.heap_base,
        layout.heap_limit,
        layout.stack_low,
        layout.stack_high,
        layout.stack_phys,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let imports: Vec<_> = blob.imports().into_iter().collect();
    bytes.extend_from_slice(&(imports.len() as u32).to_le_bytes());
    for import in imports {
        match import {
            Some(symbol) => {
                let name = symbol.as_bytes();
                let length =
                    u16::try_from(name.len()).context("PolkaVM import name is too long")?;
                bytes.extend_from_slice(&length.to_le_bytes());
                bytes.extend_from_slice(name);
            }
            None => bytes.extend_from_slice(&0u16.to_le_bytes()),
        }
    }
    let exports: Vec<_> = blob.exports().collect();
    bytes.extend_from_slice(&(exports.len() as u32).to_le_bytes());
    for export in exports {
        let name = export.symbol().as_bytes();
        let length = u16::try_from(name.len()).context("PolkaVM export name is too long")?;
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(name);
        let block = block_by_pc
            .get(&export.program_counter().0)
            .copied()
            .ok_or_else(|| anyhow!("export target is not a translated block"))?;
        bytes.extend_from_slice(&block.to_le_bytes());
    }
    Ok(bytes)
}

fn emit_gas_charge(f: &mut Function) {
    f.instruction(&W::GlobalGet(GLOBAL_GAS));
    f.instruction(&W::I64Const(1));
    f.instruction(&W::I64Sub);
    f.instruction(&W::LocalTee(LOCAL_I64_0));
    f.instruction(&W::GlobalSet(GLOBAL_GAS));
    f.instruction(&W::LocalGet(LOCAL_I64_0));
    f.instruction(&W::I64Const(0));
    f.instruction(&W::I64LeS);
    f.instruction(&W::If(BlockType::Empty));
    f.instruction(&W::I32Const(STATUS_OUT_OF_GAS));
    f.instruction(&W::Return);
    f.instruction(&W::End);
}

fn emit_block(context: &EmitContext<'_>, block: &[ParsedInstruction]) -> Result<Function> {
    let mut f = Function::new([(2, ValType::I32), (2, ValType::I64)]);
    if context.metered_targets.contains(&block[0].offset.0) {
        emit_gas_charge(&mut f);
    }

    for instruction in block {
        if emit_instruction(context, &mut f, instruction)? {
            f.instruction(&W::End);
            return Ok(f);
        }
    }
    let next = block.last().unwrap().next_offset.0;
    emit_block_target(context, &mut f, next)?;
    f.instruction(&W::End);
    Ok(f)
}

fn emit_block_target(context: &EmitContext<'_>, f: &mut Function, pc: u32) -> Result<()> {
    let target = context
        .block_by_pc
        .get(&pc)
        .copied()
        .ok_or_else(|| anyhow!("translated fallthrough target {pc} is missing"))?;
    f.instruction(&W::ReturnCall(context.block_function_base + target));
    Ok(())
}

fn emit_return_target(context: &EmitContext<'_>, f: &mut Function, pc: u32) -> Result<()> {
    emit_block_target(context, f, pc)
}

fn emit_trap(f: &mut Function, pc: u32) {
    f.instruction(&W::I32Const(pc as i32));
    f.instruction(&W::GlobalSet(GLOBAL_TRAP_PC));
    f.instruction(&W::I32Const(STATUS_TRAP));
    f.instruction(&W::Return);
}

fn emit_reg(f: &mut Function, reg: RawReg) {
    f.instruction(&W::GlobalGet(reg_index(reg)));
}

fn emit_set_reg(f: &mut Function, reg: RawReg) {
    f.instruction(&W::GlobalSet(reg_index(reg)));
}

fn emit_i32_result(f: &mut Function, reg: RawReg, operation: W<'_>) {
    f.instruction(&operation);
    f.instruction(&W::I64ExtendI32S);
    emit_set_reg(f, reg);
}

fn emit_address(f: &mut Function, base: Option<RawReg>, offset: i32) {
    if let Some(base) = base {
        emit_reg(f, base);
        f.instruction(&W::I64Const(offset as i64));
        f.instruction(&W::I64Add);
        f.instruction(&W::I32WrapI64);
    } else {
        f.instruction(&W::I32Const(offset));
    }
}

fn static_target(layout: Layout, address: u32, bytes: u32, write: bool) -> Option<(u32, u32)> {
    let end = u64::from(address).checked_add(u64::from(bytes))?;
    let (memory, base) = if !write
        && address >= layout.ro_address
        && end <= u64::from(layout.ro_address) + u64::from(layout.ro_size)
    {
        (0, layout.ro_address)
    } else if address >= layout.rw_address
        && end <= u64::from(layout.rw_address) + u64::from(layout.rw_size)
    {
        (1, layout.rw_address)
    } else if address >= layout.stack_low && end <= u64::from(layout.stack_high) {
        (2, layout.stack_low)
    } else {
        return None;
    };
    Some((memory, address.checked_sub(base)?))
}

fn emit_load_at(f: &mut Function, kind: LoadKind, memory: u32) {
    let bytes = match kind {
        LoadKind::U8 | LoadKind::I8 => 1,
        LoadKind::U16 | LoadKind::I16 => 2,
        LoadKind::U32 | LoadKind::I32 => 4,
        LoadKind::U64 => 8,
    };
    f.instruction(&match kind {
        LoadKind::U8 => W::I64Load8U(memarg(bytes, memory)),
        LoadKind::I8 => W::I64Load8S(memarg(bytes, memory)),
        LoadKind::U16 => W::I64Load16U(memarg(bytes, memory)),
        LoadKind::I16 => W::I64Load16S(memarg(bytes, memory)),
        LoadKind::U32 => W::I64Load32U(memarg(bytes, memory)),
        LoadKind::I32 => W::I64Load32S(memarg(bytes, memory)),
        LoadKind::U64 => W::I64Load(memarg(bytes, memory)),
    });
}

fn emit_load(
    context: &EmitContext<'_>,
    f: &mut Function,
    pc: u32,
    dst: RawReg,
    base: Option<RawReg>,
    offset: i32,
    kind: LoadKind,
) {
    let bytes = match kind {
        LoadKind::U8 | LoadKind::I8 => 1,
        LoadKind::U16 | LoadKind::I16 => 2,
        LoadKind::U32 | LoadKind::I32 => 4,
        LoadKind::U64 => 8,
    };
    if base.is_none() {
        if let Some((memory, physical)) = static_target(context.layout, offset as u32, bytes, false)
        {
            f.instruction(&W::I32Const(physical as i32));
            emit_load_at(f, kind, memory);
        } else {
            emit_trap(f, pc);
        }
        emit_set_reg(f, dst);
        return;
    }
    if base == Some(Reg::SP.raw()) {
        emit_address(f, base, offset);
        f.instruction(&W::I32Const(context.layout.stack_low as i32));
        f.instruction(&W::I32Sub);
        emit_load_at(f, kind, 2);
        emit_set_reg(f, dst);
        return;
    }
    emit_address(f, base, offset);
    f.instruction(&W::LocalTee(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.stack_low as i32));
    f.instruction(&W::I32GeU);
    f.instruction(&W::If(BlockType::Result(ValType::I64)));
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.stack_low as i32));
    f.instruction(&W::I32Sub);
    emit_load_at(f, kind, 2);
    f.instruction(&W::Else);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.rw_address as i32));
    f.instruction(&W::I32GeU);
    f.instruction(&W::If(BlockType::Result(ValType::I64)));
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.rw_address as i32));
    f.instruction(&W::I32Sub);
    emit_load_at(f, kind, 1);
    f.instruction(&W::Else);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.ro_address as i32));
    f.instruction(&W::I32Sub);
    emit_load_at(f, kind, 0);
    f.instruction(&W::End);
    f.instruction(&W::End);
    emit_set_reg(f, dst);
}

fn emit_store_value(
    f: &mut Function,
    kind: StoreKind,
    source: Option<RawReg>,
    immediate: i32,
    memory: u32,
) {
    if let Some(source) = source {
        emit_reg(f, source);
    } else {
        f.instruction(&W::I64Const(immediate as i64));
    }
    f.instruction(&match kind {
        StoreKind::U8 => W::I64Store8(memarg(1, memory)),
        StoreKind::U16 => W::I64Store16(memarg(2, memory)),
        StoreKind::U32 => W::I64Store32(memarg(4, memory)),
        StoreKind::U64 => W::I64Store(memarg(8, memory)),
    });
}

fn emit_store(
    context: &EmitContext<'_>,
    f: &mut Function,
    pc: u32,
    source: Option<RawReg>,
    base: Option<RawReg>,
    offset: i32,
    immediate: i32,
    kind: StoreKind,
) {
    let bytes = match kind {
        StoreKind::U8 => 1,
        StoreKind::U16 => 2,
        StoreKind::U32 => 4,
        StoreKind::U64 => 8,
    };
    if base.is_none() {
        if let Some((memory, physical)) = static_target(context.layout, offset as u32, bytes, true)
        {
            f.instruction(&W::I32Const(physical as i32));
            emit_store_value(f, kind, source, immediate, memory);
        } else {
            emit_trap(f, pc);
        }
        return;
    }
    if base == Some(Reg::SP.raw()) {
        emit_address(f, base, offset);
        f.instruction(&W::I32Const(context.layout.stack_low as i32));
        f.instruction(&W::I32Sub);
        emit_store_value(f, kind, source, immediate, 2);
        return;
    }
    emit_address(f, base, offset);
    f.instruction(&W::LocalTee(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.stack_low as i32));
    f.instruction(&W::I32GeU);
    f.instruction(&W::If(BlockType::Empty));
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.stack_low as i32));
    f.instruction(&W::I32Sub);
    emit_store_value(f, kind, source, immediate, 2);
    f.instruction(&W::Else);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.rw_address as i32));
    f.instruction(&W::I32GeU);
    f.instruction(&W::If(BlockType::Empty));
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.rw_address as i32));
    f.instruction(&W::I32Sub);
    emit_store_value(f, kind, source, immediate, 1);
    f.instruction(&W::Else);
    emit_trap(f, pc);
    f.instruction(&W::End);
    f.instruction(&W::End);
}

fn emit_binary_i64(f: &mut Function, dst: RawReg, lhs: RawReg, rhs: RawReg, operation: W<'_>) {
    emit_reg(f, lhs);
    emit_reg(f, rhs);
    f.instruction(&operation);
    emit_set_reg(f, dst);
}

fn emit_binary_i32(f: &mut Function, dst: RawReg, lhs: RawReg, rhs: RawReg, operation: W<'_>) {
    emit_reg(f, lhs);
    f.instruction(&W::I32WrapI64);
    emit_reg(f, rhs);
    f.instruction(&W::I32WrapI64);
    emit_i32_result(f, dst, operation);
}

fn emit_binary_imm_i64(f: &mut Function, dst: RawReg, lhs: RawReg, imm: i32, operation: W<'_>) {
    emit_reg(f, lhs);
    f.instruction(&W::I64Const(imm as i64));
    f.instruction(&operation);
    emit_set_reg(f, dst);
}

fn emit_binary_imm_i32(f: &mut Function, dst: RawReg, lhs: RawReg, imm: i32, operation: W<'_>) {
    emit_reg(f, lhs);
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::I32Const(imm));
    emit_i32_result(f, dst, operation);
}

#[derive(Clone, Copy)]
enum CompareOp {
    Eq,
    Ne,
    LtU,
    LtS,
    LeU,
    LeS,
    GtU,
    GtS,
    GeU,
    GeS,
}

fn emit_compare_regs(
    context: &EmitContext<'_>,
    f: &mut Function,
    lhs: RawReg,
    rhs: RawReg,
    operation: CompareOp,
) {
    emit_reg(f, lhs);
    if !context.is_64_bit {
        f.instruction(&W::I32WrapI64);
    }
    emit_reg(f, rhs);
    if !context.is_64_bit {
        f.instruction(&W::I32WrapI64);
    }
    emit_compare_operation(context, f, operation);
}

fn emit_compare_imm(
    context: &EmitContext<'_>,
    f: &mut Function,
    lhs: RawReg,
    rhs: i32,
    operation: CompareOp,
) {
    emit_reg(f, lhs);
    if context.is_64_bit {
        f.instruction(&W::I64Const(rhs as i64));
    } else {
        f.instruction(&W::I32WrapI64);
        f.instruction(&W::I32Const(rhs));
    }
    emit_compare_operation(context, f, operation);
}

fn emit_compare_operation(context: &EmitContext<'_>, f: &mut Function, operation: CompareOp) {
    f.instruction(&match (context.is_64_bit, operation) {
        (true, CompareOp::Eq) => W::I64Eq,
        (true, CompareOp::Ne) => W::I64Ne,
        (true, CompareOp::LtU) => W::I64LtU,
        (true, CompareOp::LtS) => W::I64LtS,
        (true, CompareOp::LeU) => W::I64LeU,
        (true, CompareOp::LeS) => W::I64LeS,
        (true, CompareOp::GtU) => W::I64GtU,
        (true, CompareOp::GtS) => W::I64GtS,
        (true, CompareOp::GeU) => W::I64GeU,
        (true, CompareOp::GeS) => W::I64GeS,
        (false, CompareOp::Eq) => W::I32Eq,
        (false, CompareOp::Ne) => W::I32Ne,
        (false, CompareOp::LtU) => W::I32LtU,
        (false, CompareOp::LtS) => W::I32LtS,
        (false, CompareOp::LeU) => W::I32LeU,
        (false, CompareOp::LeS) => W::I32LeS,
        (false, CompareOp::GtU) => W::I32GtU,
        (false, CompareOp::GtS) => W::I32GtS,
        (false, CompareOp::GeU) => W::I32GeU,
        (false, CompareOp::GeS) => W::I32GeS,
    });
}

fn emit_compare_branch(
    context: &EmitContext<'_>,
    f: &mut Function,
    instruction: &ParsedInstruction,
    target: u32,
) -> Result<()> {
    f.instruction(&W::If(BlockType::Empty));
    emit_block_target(context, f, target)?;
    f.instruction(&W::Else);
    emit_block_target(context, f, instruction.next_offset.0)?;
    f.instruction(&W::End);
    f.instruction(&W::Unreachable);
    Ok(())
}

fn emit_indirect(context: &EmitContext<'_>, f: &mut Function, pc: u32, base: RawReg, offset: i32) {
    emit_reg(f, base);
    f.instruction(&W::I64Const(offset as i64));
    f.instruction(&W::I64Add);
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::LocalSet(LOCAL_ADDR));
    emit_indirect_from_local(context, f, pc, base != Reg::RA.raw() || offset != 0);
}

fn emit_indirect_from_local(context: &EmitContext<'_>, f: &mut Function, pc: u32, meter: bool) {
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(RETURN_TO_HOST as i32));
    f.instruction(&W::I32Eq);
    f.instruction(&W::If(BlockType::Empty));
    f.instruction(&W::I32Const(STATUS_FINISHED));
    f.instruction(&W::Return);
    f.instruction(&W::End);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(1));
    f.instruction(&W::I32And);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Eqz);
    f.instruction(&W::I32Or);
    f.instruction(&W::If(BlockType::Empty));
    emit_trap(f, pc);
    f.instruction(&W::End);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(2));
    f.instruction(&W::I32DivU);
    f.instruction(&W::I32Const(1));
    f.instruction(&W::I32Sub);
    f.instruction(&W::LocalTee(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.jump_table_len as i32));
    f.instruction(&W::I32GeU);
    f.instruction(&W::If(BlockType::Empty));
    emit_trap(f, pc);
    f.instruction(&W::End);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.resolver_table_base as i32));
    f.instruction(&W::I32Add);
    f.instruction(&W::CallIndirect {
        type_index: TYPE_BLOCK,
        table_index: 0,
    });
    f.instruction(&W::LocalSet(LOCAL_ADDR));
    if meter {
        let current_block = context
            .block_by_pc
            .range(..=pc)
            .next_back()
            .map(|(_, index)| *index)
            .unwrap_or(0);
        f.instruction(&W::LocalGet(LOCAL_ADDR));
        f.instruction(&W::I32Const(current_block as i32));
        f.instruction(&W::I32LeU);
        f.instruction(&W::If(BlockType::Empty));
        emit_gas_charge(f);
        f.instruction(&W::End);
    } else {
        let current_block = context
            .block_by_pc
            .range(..=pc)
            .next_back()
            .map(|(_, index)| *index)
            .unwrap_or(0);
        f.instruction(&W::LocalGet(LOCAL_ADDR));
        f.instruction(&W::I32Const(current_block as i32));
        f.instruction(&W::I32Eq);
        f.instruction(&W::If(BlockType::Empty));
        emit_gas_charge(f);
        f.instruction(&W::End);
    }
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::ReturnCallIndirect {
        type_index: TYPE_BLOCK,
        table_index: 0,
    });
}

fn written_register(kind: PvmInstruction) -> Option<RawReg> {
    use PvmInstruction::*;
    Some(match kind {
        load_imm(dst, _)
        | load_imm64(dst, _)
        | move_reg(dst, _)
        | load_u8(dst, _)
        | load_i8(dst, _)
        | load_u16(dst, _)
        | load_i16(dst, _)
        | load_u32(dst, _)
        | load_i32(dst, _)
        | load_u64(dst, _)
        | load_indirect_u8(dst, _, _)
        | load_indirect_i8(dst, _, _)
        | load_indirect_u16(dst, _, _)
        | load_indirect_i16(dst, _, _)
        | load_indirect_u32(dst, _, _)
        | load_indirect_i32(dst, _, _)
        | load_indirect_u64(dst, _, _)
        | add_32(dst, _, _)
        | add_64(dst, _, _)
        | sub_32(dst, _, _)
        | sub_64(dst, _, _)
        | and(dst, _, _)
        | xor(dst, _, _)
        | or(dst, _, _)
        | and_inverted(dst, _, _)
        | or_inverted(dst, _, _)
        | xnor(dst, _, _)
        | mul_32(dst, _, _)
        | mul_64(dst, _, _)
        | mul_upper_unsigned_unsigned(dst, _, _)
        | mul_upper_signed_signed(dst, _, _)
        | mul_upper_signed_unsigned(dst, _, _)
        | add_imm_32(dst, _, _)
        | add_imm_64(dst, _, _)
        | and_imm(dst, _, _)
        | xor_imm(dst, _, _)
        | or_imm(dst, _, _)
        | mul_imm_32(dst, _, _)
        | mul_imm_64(dst, _, _)
        | negate_and_add_imm_32(dst, _, _)
        | negate_and_add_imm_64(dst, _, _)
        | set_less_than_unsigned(dst, _, _)
        | set_less_than_signed(dst, _, _)
        | set_less_than_unsigned_imm(dst, _, _)
        | set_less_than_signed_imm(dst, _, _)
        | set_greater_than_unsigned_imm(dst, _, _)
        | set_greater_than_signed_imm(dst, _, _)
        | shift_logical_left_32(dst, _, _)
        | shift_logical_left_64(dst, _, _)
        | shift_logical_right_32(dst, _, _)
        | shift_logical_right_64(dst, _, _)
        | shift_arithmetic_right_32(dst, _, _)
        | shift_arithmetic_right_64(dst, _, _)
        | shift_logical_left_imm_32(dst, _, _)
        | shift_logical_left_imm_64(dst, _, _)
        | shift_logical_right_imm_32(dst, _, _)
        | shift_logical_right_imm_64(dst, _, _)
        | shift_arithmetic_right_imm_32(dst, _, _)
        | shift_arithmetic_right_imm_64(dst, _, _)
        | shift_logical_right_imm_alt_32(dst, _, _)
        | shift_logical_right_imm_alt_64(dst, _, _)
        | shift_arithmetic_right_imm_alt_32(dst, _, _)
        | shift_arithmetic_right_imm_alt_64(dst, _, _)
        | shift_logical_left_imm_alt_32(dst, _, _)
        | shift_logical_left_imm_alt_64(dst, _, _)
        | rotate_left_32(dst, _, _)
        | rotate_left_64(dst, _, _)
        | rotate_right_32(dst, _, _)
        | rotate_right_64(dst, _, _)
        | rotate_right_imm_32(dst, _, _)
        | rotate_right_imm_64(dst, _, _)
        | rotate_right_imm_alt_32(dst, _, _)
        | rotate_right_imm_alt_64(dst, _, _)
        | div_unsigned_32(dst, _, _)
        | div_unsigned_64(dst, _, _)
        | div_signed_32(dst, _, _)
        | div_signed_64(dst, _, _)
        | rem_unsigned_32(dst, _, _)
        | rem_unsigned_64(dst, _, _)
        | rem_signed_32(dst, _, _)
        | rem_signed_64(dst, _, _)
        | count_leading_zero_bits_32(dst, _)
        | count_leading_zero_bits_64(dst, _)
        | count_trailing_zero_bits_32(dst, _)
        | count_trailing_zero_bits_64(dst, _)
        | count_set_bits_32(dst, _)
        | count_set_bits_64(dst, _)
        | sign_extend_8(dst, _)
        | sign_extend_16(dst, _)
        | zero_extend_16(dst, _)
        | reverse_byte(dst, _)
        | maximum(dst, _, _)
        | maximum_unsigned(dst, _, _)
        | minimum(dst, _, _)
        | minimum_unsigned(dst, _, _)
        | cmov_if_zero(dst, _, _)
        | cmov_if_not_zero(dst, _, _)
        | cmov_if_zero_imm(dst, _, _)
        | cmov_if_not_zero_imm(dst, _, _)
        | sbrk(dst, _) => dst,
        _ => return None,
    })
}

fn normalize_32(f: &mut Function, reg: RawReg) {
    emit_reg(f, reg);
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::I64ExtendI32U);
    emit_set_reg(f, reg);
}

fn emit_instruction(
    context: &EmitContext<'_>,
    f: &mut Function,
    instruction: &ParsedInstruction,
) -> Result<bool> {
    use PvmInstruction::*;
    let pc = instruction.offset.0;
    match instruction.kind {
        trap | invalid => {
            emit_trap(f, pc);
            return Ok(true);
        }
        fallthrough | unlikely => {}
        memset => emit_memset(context, f, pc),
        jump(target) => {
            emit_return_target(context, f, target)?;
            return Ok(true);
        }
        jump_indirect(base, offset) => {
            emit_indirect(context, f, pc, base, offset);
            return Ok(true);
        }
        load_imm_and_jump(dst, value, target) => {
            f.instruction(&W::I64Const(if context.is_64_bit {
                value as i64
            } else {
                value as u32 as i64
            }));
            emit_set_reg(f, dst);
            emit_return_target(context, f, target)?;
            return Ok(true);
        }
        load_imm_and_jump_indirect(dst, base, value, offset) => {
            // The target address uses the old base value when dst aliases base.
            emit_reg(f, base);
            f.instruction(&W::I64Const(offset as i64));
            f.instruction(&W::I64Add);
            f.instruction(&W::I32WrapI64);
            f.instruction(&W::LocalSet(LOCAL_ADDR));
            f.instruction(&W::I64Const(if context.is_64_bit {
                value as i64
            } else {
                value as u32 as i64
            }));
            emit_set_reg(f, dst);
            emit_indirect_from_local(context, f, pc, true);
            return Ok(true);
        }
        ecalli(index) => {
            f.instruction(&W::I32Const(index));
            f.instruction(&W::GlobalSet(GLOBAL_ECALL));
            let next = context
                .block_by_pc
                .get(&instruction.next_offset.0)
                .copied()
                .ok_or_else(|| anyhow!("ecall continuation is not a block"))?;
            f.instruction(&W::I32Const(next as i32));
            f.instruction(&W::GlobalSet(GLOBAL_PC));
            f.instruction(&W::I32Const(STATUS_ECALL));
            f.instruction(&W::Return);
            return Ok(true);
        }
        load_imm(dst, value) => {
            f.instruction(&W::I64Const(value as i64));
            emit_set_reg(f, dst);
        }
        load_imm64(dst, value) => {
            f.instruction(&W::I64Const(value as i64));
            emit_set_reg(f, dst);
        }
        move_reg(dst, src) => {
            emit_reg(f, src);
            emit_set_reg(f, dst);
        }
        load_u8(dst, offset) => emit_load(context, f, pc, dst, None, offset, LoadKind::U8),
        load_i8(dst, offset) => emit_load(context, f, pc, dst, None, offset, LoadKind::I8),
        load_u16(dst, offset) => emit_load(context, f, pc, dst, None, offset, LoadKind::U16),
        load_i16(dst, offset) => emit_load(context, f, pc, dst, None, offset, LoadKind::I16),
        load_u32(dst, offset) => emit_load(context, f, pc, dst, None, offset, LoadKind::U32),
        load_i32(dst, offset) => emit_load(context, f, pc, dst, None, offset, LoadKind::I32),
        load_u64(dst, offset) => emit_load(context, f, pc, dst, None, offset, LoadKind::U64),
        load_indirect_u8(dst, base, offset) => {
            emit_load(context, f, pc, dst, Some(base), offset, LoadKind::U8)
        }
        load_indirect_i8(dst, base, offset) => {
            emit_load(context, f, pc, dst, Some(base), offset, LoadKind::I8)
        }
        load_indirect_u16(dst, base, offset) => {
            emit_load(context, f, pc, dst, Some(base), offset, LoadKind::U16)
        }
        load_indirect_i16(dst, base, offset) => {
            emit_load(context, f, pc, dst, Some(base), offset, LoadKind::I16)
        }
        load_indirect_u32(dst, base, offset) => {
            emit_load(context, f, pc, dst, Some(base), offset, LoadKind::U32)
        }
        load_indirect_i32(dst, base, offset) => {
            emit_load(context, f, pc, dst, Some(base), offset, LoadKind::I32)
        }
        load_indirect_u64(dst, base, offset) => {
            emit_load(context, f, pc, dst, Some(base), offset, LoadKind::U64)
        }
        store_u8(src, offset) => {
            emit_store(context, f, pc, Some(src), None, offset, 0, StoreKind::U8)
        }
        store_u16(src, offset) => {
            emit_store(context, f, pc, Some(src), None, offset, 0, StoreKind::U16)
        }
        store_u32(src, offset) => {
            emit_store(context, f, pc, Some(src), None, offset, 0, StoreKind::U32)
        }
        store_u64(src, offset) => {
            emit_store(context, f, pc, Some(src), None, offset, 0, StoreKind::U64)
        }
        store_indirect_u8(src, base, offset) => emit_store(
            context,
            f,
            pc,
            Some(src),
            Some(base),
            offset,
            0,
            StoreKind::U8,
        ),
        store_indirect_u16(src, base, offset) => emit_store(
            context,
            f,
            pc,
            Some(src),
            Some(base),
            offset,
            0,
            StoreKind::U16,
        ),
        store_indirect_u32(src, base, offset) => emit_store(
            context,
            f,
            pc,
            Some(src),
            Some(base),
            offset,
            0,
            StoreKind::U32,
        ),
        store_indirect_u64(src, base, offset) => emit_store(
            context,
            f,
            pc,
            Some(src),
            Some(base),
            offset,
            0,
            StoreKind::U64,
        ),
        store_imm_u8(offset, value) => {
            emit_store(context, f, pc, None, None, offset, value, StoreKind::U8)
        }
        store_imm_u16(offset, value) => {
            emit_store(context, f, pc, None, None, offset, value, StoreKind::U16)
        }
        store_imm_u32(offset, value) => {
            emit_store(context, f, pc, None, None, offset, value, StoreKind::U32)
        }
        store_imm_u64(offset, value) => {
            emit_store(context, f, pc, None, None, offset, value, StoreKind::U64)
        }
        store_imm_indirect_u8(base, offset, value) => emit_store(
            context,
            f,
            pc,
            None,
            Some(base),
            offset,
            value,
            StoreKind::U8,
        ),
        store_imm_indirect_u16(base, offset, value) => emit_store(
            context,
            f,
            pc,
            None,
            Some(base),
            offset,
            value,
            StoreKind::U16,
        ),
        store_imm_indirect_u32(base, offset, value) => emit_store(
            context,
            f,
            pc,
            None,
            Some(base),
            offset,
            value,
            StoreKind::U32,
        ),
        store_imm_indirect_u64(base, offset, value) => emit_store(
            context,
            f,
            pc,
            None,
            Some(base),
            offset,
            value,
            StoreKind::U64,
        ),
        add_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32Add),
        add_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Add),
        sub_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32Sub),
        sub_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Sub),
        and(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64And),
        xor(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Xor),
        or(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Or),
        and_inverted(dst, a, b) => {
            emit_reg(f, a);
            emit_reg(f, b);
            f.instruction(&W::I64Const(-1));
            f.instruction(&W::I64Xor);
            f.instruction(&W::I64And);
            emit_set_reg(f, dst);
        }
        or_inverted(dst, a, b) => {
            emit_reg(f, a);
            emit_reg(f, b);
            f.instruction(&W::I64Const(-1));
            f.instruction(&W::I64Xor);
            f.instruction(&W::I64Or);
            emit_set_reg(f, dst);
        }
        xnor(dst, a, b) => {
            emit_reg(f, a);
            emit_reg(f, b);
            f.instruction(&W::I64Xor);
            f.instruction(&W::I64Const(-1));
            f.instruction(&W::I64Xor);
            emit_set_reg(f, dst);
        }
        mul_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32Mul),
        mul_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Mul),
        mul_upper_unsigned_unsigned(dst, a, b) => {
            emit_mul_high(context, f, dst, a, b, false, false);
        }
        mul_upper_signed_signed(dst, a, b) => emit_mul_high(context, f, dst, a, b, true, true),
        mul_upper_signed_unsigned(dst, a, b) => emit_mul_high(context, f, dst, a, b, true, false),
        add_imm_32(dst, a, imm) => emit_binary_imm_i32(f, dst, a, imm, W::I32Add),
        add_imm_64(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64Add),
        and_imm(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64And),
        xor_imm(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64Xor),
        or_imm(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64Or),
        mul_imm_32(dst, a, imm) => emit_binary_imm_i32(f, dst, a, imm, W::I32Mul),
        mul_imm_64(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64Mul),
        negate_and_add_imm_32(dst, a, imm) => {
            f.instruction(&W::I32Const(imm));
            emit_reg(f, a);
            f.instruction(&W::I32WrapI64);
            emit_i32_result(f, dst, W::I32Sub);
        }
        negate_and_add_imm_64(dst, a, imm) => {
            f.instruction(&W::I64Const(imm as i64));
            emit_reg(f, a);
            f.instruction(&W::I64Sub);
            emit_set_reg(f, dst);
        }
        set_less_than_unsigned(dst, a, b) => emit_comparison(context, f, dst, a, b, CompareOp::LtU),
        set_less_than_signed(dst, a, b) => emit_comparison(context, f, dst, a, b, CompareOp::LtS),
        set_less_than_unsigned_imm(dst, a, imm) => {
            emit_comparison_imm(context, f, dst, a, imm, CompareOp::LtU)
        }
        set_less_than_signed_imm(dst, a, imm) => {
            emit_comparison_imm(context, f, dst, a, imm, CompareOp::LtS)
        }
        set_greater_than_unsigned_imm(dst, a, imm) => {
            emit_comparison_imm(context, f, dst, a, imm, CompareOp::GtU)
        }
        set_greater_than_signed_imm(dst, a, imm) => {
            emit_comparison_imm(context, f, dst, a, imm, CompareOp::GtS)
        }
        shift_logical_left_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32Shl),
        shift_logical_left_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Shl),
        shift_logical_right_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32ShrU),
        shift_logical_right_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64ShrU),
        shift_arithmetic_right_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32ShrS),
        shift_arithmetic_right_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64ShrS),
        shift_logical_left_imm_32(dst, a, imm) => emit_binary_imm_i32(f, dst, a, imm, W::I32Shl),
        shift_logical_left_imm_64(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64Shl),
        shift_logical_right_imm_32(dst, a, imm) => emit_binary_imm_i32(f, dst, a, imm, W::I32ShrU),
        shift_logical_right_imm_64(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64ShrU),
        shift_arithmetic_right_imm_32(dst, a, imm) => {
            emit_binary_imm_i32(f, dst, a, imm, W::I32ShrS)
        }
        shift_arithmetic_right_imm_64(dst, a, imm) => {
            emit_binary_imm_i64(f, dst, a, imm, W::I64ShrS)
        }
        shift_logical_right_imm_alt_32(dst, a, imm) => {
            emit_alt_shift_i32(f, dst, a, imm, W::I32ShrU)
        }
        shift_logical_right_imm_alt_64(dst, a, imm) => {
            emit_alt_shift_i64(f, dst, a, imm, W::I64ShrU)
        }
        shift_arithmetic_right_imm_alt_32(dst, a, imm) => {
            emit_alt_shift_i32(f, dst, a, imm, W::I32ShrS)
        }
        shift_arithmetic_right_imm_alt_64(dst, a, imm) => {
            emit_alt_shift_i64(f, dst, a, imm, W::I64ShrS)
        }
        shift_logical_left_imm_alt_32(dst, a, imm) => emit_alt_shift_i32(f, dst, a, imm, W::I32Shl),
        shift_logical_left_imm_alt_64(dst, a, imm) => emit_alt_shift_i64(f, dst, a, imm, W::I64Shl),
        rotate_left_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32Rotl),
        rotate_left_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Rotl),
        rotate_right_32(dst, a, b) => emit_binary_i32(f, dst, a, b, W::I32Rotr),
        rotate_right_64(dst, a, b) => emit_binary_i64(f, dst, a, b, W::I64Rotr),
        rotate_right_imm_32(dst, a, imm) => emit_binary_imm_i32(f, dst, a, imm, W::I32Rotr),
        rotate_right_imm_64(dst, a, imm) => emit_binary_imm_i64(f, dst, a, imm, W::I64Rotr),
        rotate_right_imm_alt_32(dst, a, imm) => emit_alt_shift_i32(f, dst, a, imm, W::I32Rotr),
        rotate_right_imm_alt_64(dst, a, imm) => emit_alt_shift_i64(f, dst, a, imm, W::I64Rotr),
        div_unsigned_32(dst, a, b) => emit_divrem(f, dst, a, b, false, false, true),
        div_unsigned_64(dst, a, b) => emit_divrem(f, dst, a, b, false, false, false),
        div_signed_32(dst, a, b) => emit_divrem(f, dst, a, b, true, false, true),
        div_signed_64(dst, a, b) => emit_divrem(f, dst, a, b, true, false, false),
        rem_unsigned_32(dst, a, b) => emit_divrem(f, dst, a, b, false, true, true),
        rem_unsigned_64(dst, a, b) => emit_divrem(f, dst, a, b, false, true, false),
        rem_signed_32(dst, a, b) => emit_divrem(f, dst, a, b, true, true, true),
        rem_signed_64(dst, a, b) => emit_divrem(f, dst, a, b, true, true, false),
        count_leading_zero_bits_32(dst, src) => emit_unary_i32(f, dst, src, W::I32Clz),
        count_leading_zero_bits_64(dst, src) => emit_unary_i64(f, dst, src, W::I64Clz),
        count_trailing_zero_bits_32(dst, src) => emit_unary_i32(f, dst, src, W::I32Ctz),
        count_trailing_zero_bits_64(dst, src) => emit_unary_i64(f, dst, src, W::I64Ctz),
        count_set_bits_32(dst, src) => emit_unary_i32(f, dst, src, W::I32Popcnt),
        count_set_bits_64(dst, src) => emit_unary_i64(f, dst, src, W::I64Popcnt),
        sign_extend_8(dst, src) => {
            emit_reg(f, src);
            f.instruction(&W::I64Extend8S);
            emit_set_reg(f, dst);
        }
        sign_extend_16(dst, src) => {
            emit_reg(f, src);
            f.instruction(&W::I64Extend16S);
            emit_set_reg(f, dst);
        }
        zero_extend_16(dst, src) => {
            emit_reg(f, src);
            f.instruction(&W::I64Const(0xffff));
            f.instruction(&W::I64And);
            emit_set_reg(f, dst);
        }
        reverse_byte(dst, src) => {
            emit_reg(f, src);
            f.instruction(&W::Call(if context.is_64_bit {
                context.bswap64_function
            } else {
                context.bswap32_function
            }));
            emit_set_reg(f, dst);
        }
        maximum(dst, a, b) => emit_minmax(f, dst, a, b, true, false, context.is_64_bit),
        maximum_unsigned(dst, a, b) => emit_minmax(f, dst, a, b, true, true, context.is_64_bit),
        minimum(dst, a, b) => emit_minmax(f, dst, a, b, false, false, context.is_64_bit),
        minimum_unsigned(dst, a, b) => emit_minmax(f, dst, a, b, false, true, context.is_64_bit),
        cmov_if_zero(dst, src, condition) => emit_cmov(f, dst, src, condition, true),
        cmov_if_not_zero(dst, src, condition) => emit_cmov(f, dst, src, condition, false),
        cmov_if_zero_imm(dst, condition, value) => emit_cmov_imm(f, dst, condition, value, true),
        cmov_if_not_zero_imm(dst, condition, value) => {
            emit_cmov_imm(f, dst, condition, value, false)
        }
        sbrk(dst, size) => emit_sbrk(context, f, dst, size),
        branch_eq(a, b, target) => {
            emit_compare_regs(context, f, a, b, CompareOp::Eq);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_not_eq(a, b, target) => {
            emit_compare_regs(context, f, a, b, CompareOp::Ne);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_less_unsigned(a, b, target) => {
            emit_compare_regs(context, f, a, b, CompareOp::LtU);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_less_signed(a, b, target) => {
            emit_compare_regs(context, f, a, b, CompareOp::LtS);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_greater_or_equal_unsigned(a, b, target) => {
            emit_compare_regs(context, f, a, b, CompareOp::GeU);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_greater_or_equal_signed(a, b, target) => {
            emit_compare_regs(context, f, a, b, CompareOp::GeS);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_eq_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::Eq);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_not_eq_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::Ne);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_less_unsigned_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::LtU);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_less_signed_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::LtS);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_greater_or_equal_unsigned_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::GeU);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_greater_or_equal_signed_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::GeS);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_less_or_equal_unsigned_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::LeU);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_less_or_equal_signed_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::LeS);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_greater_unsigned_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::GtU);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
        branch_greater_signed_imm(a, imm, target) => {
            emit_compare_imm(context, f, a, imm, CompareOp::GtS);
            emit_compare_branch(context, f, instruction, target)?;
            return Ok(true);
        }
    }
    if !context.is_64_bit {
        if instruction.kind == PvmInstruction::memset {
            normalize_32(f, Reg::A0.raw());
            normalize_32(f, Reg::A2.raw());
        } else if let Some(reg) = written_register(instruction.kind) {
            normalize_32(f, reg);
        }
    }
    Ok(false)
}

fn emit_memset(context: &EmitContext<'_>, f: &mut Function, pc: u32) {
    emit_reg(f, Reg::A0.raw());
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::LocalSet(LOCAL_ADDR));
    emit_reg(f, Reg::A2.raw());
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::LocalSet(LOCAL_PHYS));
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.stack_low as i32));
    f.instruction(&W::I32GeU);
    f.instruction(&W::If(BlockType::Empty));
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.stack_low as i32));
    f.instruction(&W::I32Sub);
    emit_reg(f, Reg::A1.raw());
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::LocalGet(LOCAL_PHYS));
    f.instruction(&W::MemoryFill(2));
    f.instruction(&W::Else);
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.rw_address as i32));
    f.instruction(&W::I32GeU);
    f.instruction(&W::If(BlockType::Empty));
    f.instruction(&W::LocalGet(LOCAL_ADDR));
    f.instruction(&W::I32Const(context.layout.rw_address as i32));
    f.instruction(&W::I32Sub);
    emit_reg(f, Reg::A1.raw());
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::LocalGet(LOCAL_PHYS));
    f.instruction(&W::MemoryFill(1));
    f.instruction(&W::Else);
    emit_trap(f, pc);
    f.instruction(&W::End);
    f.instruction(&W::End);
    emit_reg(f, Reg::A0.raw());
    emit_reg(f, Reg::A2.raw());
    f.instruction(&W::I64Add);
    emit_set_reg(f, Reg::A0.raw());
    f.instruction(&W::I64Const(0));
    emit_set_reg(f, Reg::A2.raw());
}

fn emit_mul_high(
    context: &EmitContext<'_>,
    f: &mut Function,
    dst: RawReg,
    a: RawReg,
    b: RawReg,
    signed_a: bool,
    signed_b: bool,
) {
    if !context.is_64_bit {
        emit_reg(f, a);
        f.instruction(&W::I32WrapI64);
        f.instruction(&if signed_a {
            W::I64ExtendI32S
        } else {
            W::I64ExtendI32U
        });
        emit_reg(f, b);
        f.instruction(&W::I32WrapI64);
        f.instruction(&if signed_b {
            W::I64ExtendI32S
        } else {
            W::I64ExtendI32U
        });
        f.instruction(&W::I64Mul);
        f.instruction(&W::I64Const(32));
        f.instruction(&W::I64ShrU);
        emit_set_reg(f, dst);
        return;
    }
    emit_reg(f, a);
    f.instruction(&W::LocalSet(LOCAL_I64_0));
    emit_reg(f, b);
    f.instruction(&W::LocalSet(LOCAL_I64_1));
    f.instruction(&W::LocalGet(LOCAL_I64_0));
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    f.instruction(&W::Call(context.mulhu_function));
    if signed_a {
        f.instruction(&W::LocalGet(LOCAL_I64_0));
        f.instruction(&W::I64Const(0));
        f.instruction(&W::I64LtS);
        f.instruction(&W::If(BlockType::Result(ValType::I64)));
        f.instruction(&W::LocalGet(LOCAL_I64_1));
        f.instruction(&W::Else);
        f.instruction(&W::I64Const(0));
        f.instruction(&W::End);
        f.instruction(&W::I64Sub);
    }
    if signed_b {
        f.instruction(&W::LocalGet(LOCAL_I64_1));
        f.instruction(&W::I64Const(0));
        f.instruction(&W::I64LtS);
        f.instruction(&W::If(BlockType::Result(ValType::I64)));
        f.instruction(&W::LocalGet(LOCAL_I64_0));
        f.instruction(&W::Else);
        f.instruction(&W::I64Const(0));
        f.instruction(&W::End);
        f.instruction(&W::I64Sub);
    }
    emit_set_reg(f, dst);
}

fn emit_comparison(
    context: &EmitContext<'_>,
    f: &mut Function,
    dst: RawReg,
    a: RawReg,
    b: RawReg,
    operation: CompareOp,
) {
    emit_compare_regs(context, f, a, b, operation);
    f.instruction(&W::I64ExtendI32U);
    emit_set_reg(f, dst);
}

fn emit_comparison_imm(
    context: &EmitContext<'_>,
    f: &mut Function,
    dst: RawReg,
    a: RawReg,
    imm: i32,
    operation: CompareOp,
) {
    emit_compare_imm(context, f, a, imm, operation);
    f.instruction(&W::I64ExtendI32U);
    emit_set_reg(f, dst);
}
fn emit_alt_shift_i32(f: &mut Function, dst: RawReg, a: RawReg, imm: i32, operation: W<'_>) {
    f.instruction(&W::I32Const(imm));
    emit_reg(f, a);
    f.instruction(&W::I32WrapI64);
    emit_i32_result(f, dst, operation);
}
fn emit_alt_shift_i64(f: &mut Function, dst: RawReg, a: RawReg, imm: i32, operation: W<'_>) {
    f.instruction(&W::I64Const(imm as i64));
    emit_reg(f, a);
    f.instruction(&operation);
    emit_set_reg(f, dst);
}
fn emit_unary_i32(f: &mut Function, dst: RawReg, src: RawReg, operation: W<'_>) {
    emit_reg(f, src);
    f.instruction(&W::I32WrapI64);
    emit_i32_result(f, dst, operation);
}
fn emit_unary_i64(f: &mut Function, dst: RawReg, src: RawReg, operation: W<'_>) {
    emit_reg(f, src);
    f.instruction(&operation);
    emit_set_reg(f, dst);
}

fn emit_divrem(
    f: &mut Function,
    dst: RawReg,
    a: RawReg,
    b: RawReg,
    signed: bool,
    remainder: bool,
    bits32: bool,
) {
    emit_reg(f, a);
    f.instruction(&W::LocalSet(LOCAL_I64_0));
    emit_reg(f, b);
    f.instruction(&W::LocalSet(LOCAL_I64_1));
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    if bits32 {
        f.instruction(&W::I32WrapI64);
        f.instruction(&W::I32Eqz);
    } else {
        f.instruction(&W::I64Eqz);
    }
    f.instruction(&W::If(BlockType::Result(ValType::I64)));
    if remainder {
        f.instruction(&W::LocalGet(LOCAL_I64_0));
        if bits32 {
            f.instruction(&W::I32WrapI64);
            f.instruction(&W::I64ExtendI32S);
        }
    } else {
        f.instruction(&W::I64Const(-1));
    }
    f.instruction(&W::Else);
    if signed {
        f.instruction(&W::LocalGet(LOCAL_I64_0));
        if bits32 {
            f.instruction(&W::I32WrapI64);
            f.instruction(&W::I32Const(i32::MIN));
            f.instruction(&W::I32Eq);
            f.instruction(&W::LocalGet(LOCAL_I64_1));
            f.instruction(&W::I32WrapI64);
            f.instruction(&W::I32Const(-1));
            f.instruction(&W::I32Eq);
        } else {
            f.instruction(&W::I64Const(i64::MIN));
            f.instruction(&W::I64Eq);
            f.instruction(&W::LocalGet(LOCAL_I64_1));
            f.instruction(&W::I64Const(-1));
            f.instruction(&W::I64Eq);
        }
        f.instruction(&W::I32And);
        f.instruction(&W::If(BlockType::Result(ValType::I64)));
        f.instruction(&W::I64Const(if remainder {
            0
        } else if bits32 {
            i32::MIN as i64
        } else {
            i64::MIN
        }));
        f.instruction(&W::Else);
        emit_divrem_operation(f, signed, remainder, bits32);
        f.instruction(&W::End);
    } else {
        emit_divrem_operation(f, signed, remainder, bits32);
    }
    f.instruction(&W::End);
    emit_set_reg(f, dst);
}

fn emit_divrem_operation(f: &mut Function, signed: bool, remainder: bool, bits32: bool) {
    f.instruction(&W::LocalGet(LOCAL_I64_0));
    if bits32 {
        f.instruction(&W::I32WrapI64);
    }
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    if bits32 {
        f.instruction(&W::I32WrapI64);
        f.instruction(&match (signed, remainder) {
            (true, true) => W::I32RemS,
            (true, false) => W::I32DivS,
            (false, true) => W::I32RemU,
            (false, false) => W::I32DivU,
        });
        f.instruction(&W::I64ExtendI32S);
    } else {
        f.instruction(&match (signed, remainder) {
            (true, true) => W::I64RemS,
            (true, false) => W::I64DivS,
            (false, true) => W::I64RemU,
            (false, false) => W::I64DivU,
        });
    }
}

fn emit_minmax(
    f: &mut Function,
    dst: RawReg,
    a: RawReg,
    b: RawReg,
    maximum: bool,
    unsigned: bool,
    bits64: bool,
) {
    emit_reg(f, a);
    f.instruction(&W::LocalSet(LOCAL_I64_0));
    emit_reg(f, b);
    f.instruction(&W::LocalSet(LOCAL_I64_1));
    f.instruction(&W::LocalGet(LOCAL_I64_0));
    if !bits64 {
        f.instruction(&W::I32WrapI64);
    }
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    if !bits64 {
        f.instruction(&W::I32WrapI64);
    }
    f.instruction(&match (maximum, unsigned, bits64) {
        (true, true, true) => W::I64GtU,
        (true, false, true) => W::I64GtS,
        (false, true, true) => W::I64LtU,
        (false, false, true) => W::I64LtS,
        (true, true, false) => W::I32GtU,
        (true, false, false) => W::I32GtS,
        (false, true, false) => W::I32LtU,
        (false, false, false) => W::I32LtS,
    });
    f.instruction(&W::If(BlockType::Result(ValType::I64)));
    f.instruction(&W::LocalGet(LOCAL_I64_0));
    f.instruction(&W::Else);
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    f.instruction(&W::End);
    if !bits64 {
        f.instruction(&W::I32WrapI64);
        f.instruction(&W::I64ExtendI32S);
    }
    emit_set_reg(f, dst);
}

fn emit_cmov(f: &mut Function, dst: RawReg, src: RawReg, condition: RawReg, zero: bool) {
    emit_reg(f, condition);
    f.instruction(&W::I64Eqz);
    if !zero {
        f.instruction(&W::I32Eqz);
    }
    f.instruction(&W::If(BlockType::Empty));
    emit_reg(f, src);
    emit_set_reg(f, dst);
    f.instruction(&W::End);
}
fn emit_cmov_imm(f: &mut Function, dst: RawReg, condition: RawReg, value: i32, zero: bool) {
    emit_reg(f, condition);
    f.instruction(&W::I64Eqz);
    if !zero {
        f.instruction(&W::I32Eqz);
    }
    f.instruction(&W::If(BlockType::Empty));
    f.instruction(&W::I64Const(value as i64));
    emit_set_reg(f, dst);
    f.instruction(&W::End);
}

fn emit_sbrk(context: &EmitContext<'_>, f: &mut Function, dst: RawReg, size: RawReg) {
    emit_reg(f, size);
    f.instruction(&W::LocalSet(LOCAL_I64_0));
    if context.is_64_bit {
        f.instruction(&W::LocalGet(LOCAL_I64_0));
        f.instruction(&W::I64Const(u32::MAX as i64));
        f.instruction(&W::I64GtU);
        f.instruction(&W::If(BlockType::Empty));
        f.instruction(&W::I64Const(0));
        f.instruction(&W::LocalSet(LOCAL_I64_0));
        f.instruction(&W::End);
    }
    f.instruction(&W::GlobalGet(GLOBAL_HEAP_SIZE));
    f.instruction(&W::LocalGet(LOCAL_I64_0));
    f.instruction(&W::I64Add);
    f.instruction(&W::LocalTee(LOCAL_I64_1));
    f.instruction(&W::I64Const(context.layout.heap_limit as i64));
    f.instruction(&W::I64LeU);
    f.instruction(&W::If(BlockType::Result(ValType::I64)));
    f.instruction(&W::I64Const(
        context
            .layout
            .heap_base
            .wrapping_sub(context.layout.rw_address) as i64,
    ));
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    f.instruction(&W::I64Add);
    f.instruction(&W::I64Const((PAGE_SIZE - 1) as i64));
    f.instruction(&W::I64Add);
    f.instruction(&W::I64Const(PAGE_SIZE.trailing_zeros() as i64));
    f.instruction(&W::I64ShrU);
    f.instruction(&W::I32WrapI64);
    f.instruction(&W::MemorySize(1));
    f.instruction(&W::I32Sub);
    f.instruction(&W::MemoryGrow(1));
    f.instruction(&W::I32Const(-1));
    f.instruction(&W::I32Ne);
    f.instruction(&W::If(BlockType::Result(ValType::I64)));
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    f.instruction(&W::GlobalSet(GLOBAL_HEAP_SIZE));
    f.instruction(&W::I64Const(context.layout.heap_base as i64));
    f.instruction(&W::LocalGet(LOCAL_I64_1));
    f.instruction(&W::I64Add);
    f.instruction(&W::Else);
    f.instruction(&W::I64Const(0));
    f.instruction(&W::End);
    f.instruction(&W::Else);
    f.instruction(&W::I64Const(0));
    f.instruction(&W::End);
    emit_set_reg(f, dst);
}

#[cfg(test)]
mod tests {
    use super::{translate, STATUS_FINISHED};
    use polkavm::program::{assemble, InstructionSetKind};
    use polkavm::{
        BackendKind, Config, Engine, InterruptKind, Module, ModuleConfig, ProgramBlob, Reg,
        RETURN_TO_HOST,
    };
    use wasmi::{Engine as WasmEngine, Linker, Module as WasmModule, Store, Val};

    fn interpreter_registers(program: &[u8]) -> [u64; 13] {
        let blob = ProgramBlob::parse(program.into()).expect("parse differential fixture");
        let entry = blob
            .exports()
            .find(|export| export.symbol() == "main")
            .expect("main export")
            .program_counter();
        let mut config = Config::new();
        config.set_backend(Some(BackendKind::Interpreter));
        config.set_sandboxing_enabled(true);
        let engine = Engine::new(&config).expect("create interpreter");
        let module =
            Module::from_blob(&engine, &ModuleConfig::new(), blob).expect("compile fixture");
        let mut instance = module.instantiate().expect("instantiate fixture");
        instance.set_reg(Reg::SP, module.default_sp());
        instance.set_reg(Reg::RA, RETURN_TO_HOST.into());
        instance.set_gas(i64::MAX);
        instance.set_next_program_counter(entry);
        assert_eq!(
            instance.run().expect("run interpreter"),
            InterruptKind::Finished
        );
        Reg::ALL.map(|reg| instance.reg(reg))
    }

    fn translated_registers(program: &[u8]) -> [u64; 13] {
        let wasm = translate(program).expect("translate differential fixture");
        let engine = WasmEngine::default();
        let module = WasmModule::new(&engine, &wasm[..]).expect("compile translated fixture");
        let mut store = Store::new(&engine, ());
        let linker = Linker::new(&engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("instantiate translated fixture")
            .start(&mut store)
            .expect("start translated fixture");
        let begin = instance
            .get_typed_func::<(i32, i64), i32>(&store, "pvm_begin")
            .expect("translated begin export");
        assert_eq!(
            begin
                .call(&mut store, (0, i64::MAX))
                .expect("run translated fixture"),
            STATUS_FINISHED
        );
        std::array::from_fn(|index| {
            let value = instance
                .get_global(&store, &format!("r{index}"))
                .expect("translated register")
                .get(&store);
            let Val::I64(value) = value else {
                panic!("translated register is not i64");
            };
            value as u64
        })
    }

    fn assert_differential_isa(isa: InstructionSetKind, source: &str) {
        let program = assemble(Some(isa), source).expect("assemble differential fixture");
        assert_eq!(
            translated_registers(&program),
            interpreter_registers(&program)
        );
    }

    fn assert_differential(source: &str) {
        assert_differential_isa(InstructionSetKind::Latest64, source);
    }

    #[test]
    fn framebuffer_fixture_translates_to_valid_wasm() {
        let program = include_bytes!("../tests/fixtures/framebuffer-test.polkavm");
        let wasm = translate(program).expect("translate framebuffer fixture");
        wasmparser::Validator::new()
            .validate_all(&wasm)
            .expect("validate translated framebuffer fixture");
    }

    #[test]
    fn arithmetic_matches_interpreter() {
        assert_differential(
            r#"
                %stack_size = 4096
                pub @main:
                a0 = 1234
                a1 = 100
                a2 = a0 + a1
                a3 = a2 * 7
                a4 = a3 /u a1
                a5 = a3 %u a1
                ret
            "#,
        );
    }

    #[test]
    fn reverse_bytes_match_interpreter() {
        let source = r#"
            %stack_size = 4096
            pub @main:
            a0 = 305419896
            a1 = reverse a0
            ret
        "#;
        assert_differential_isa(InstructionSetKind::Latest64, source);
        assert_differential_isa(InstructionSetKind::Latest32, source);
    }

    #[test]
    fn division_edges_match_interpreter() {
        assert_differential(
            r#"
                %stack_size = 4096
                pub @main:
                a0 = -2147483648
                a1 = -1
                i32 a2 = a0 /s a1
                i32 a3 = a0 %s a1
                a4 = 0
                a5 = a0 /u a4
                ret
            "#,
        );
    }

    #[test]
    fn memory_and_branches_match_interpreter() {
        assert_differential(
            r#"
                %rw_data_size = 65536
                %stack_size = 4096
                pub @main:
                a0 = 305419896
                u64 [131072] = a0
                a1 = u8 [131072]
                a2 = u16 [131072]
                a3 = i32 [131072]
                a4 = u64 [131072]
                jump @matched if a1 == 120
                a5 = -1
                ret
                @matched:
                a5 = a4
                ret
            "#,
        );
    }

    #[test]
    fn latest32_unsigned_control_flow_matches_interpreter() {
        assert_differential_isa(
            InstructionSetKind::Latest32,
            r#"
                %stack_size = 4096
                pub @main:
                a0 = -1
                a1 = 1
                i32 a2 = a0 + a1
                jump @wrong if a0 <u a1
                a3 = 7
                ret
                @wrong:
                a3 = 9
                ret
            "#,
        );
    }
}
