/**
 * Products Demo - Counter Example (React)
 *
 * Demonstrates reactive state with React's useState via the custom reconciler.
 * Uses the product SDK directly for messaging and action handling.
 */

import { useState, useEffect, useRef } from 'react';
import { Column, Row, Spacer, Text, Button, TextField, registerChatMessageRenderer } from '@novasamatech/product-react-renderer';
import { createClient, type PolkadotSigner, type TxEvent } from 'polkadot-api';
import { pop, assetHub, MultiAddress, XcmV5Junctions, XcmV5Junction } from '@polkadot-api/descriptors';
import { createAccountsProvider, createPapiProvider, createProductChatManager, createStatementStore, deriveEntropy, hostApi, notificationManager, paymentManager, preimageManager, requestDevicePermission, ringVrfKeyHandle, type ProofContext, type RegisteredRingVrfKey, type RingVrfKeyDisclosure, type RingVrfKeyHandle, type PaymentStatus, type SignedStatement, type TopUpStatus } from '@novasamatech/host-api-wrapper';
import { enumValue, fromHex, RingLocation } from '@novasamatech/host-api';
import type { CodecType } from 'scale-ts';
import { fromBufferToBase58, getSs58AddressInfo } from '@polkadot-api/substrate-bindings';
import { Keyring } from '@polkadot/keyring';

// ============================================================================
// Message Data Types
// ============================================================================

interface CounterData {
    type: 'counter';
    initialCount: number;
}

interface BalanceData {
    type: 'balance';
}

interface TransferData {
    type: 'transfer';
}

interface SignRawData {
    type: 'signRaw';
}

interface RingVrfData {
    type: 'ringVrf';
}

interface CreateRoomFormData {
    type: 'createRoomForm';
}

interface NotificationTestData {
    type: 'notificationTest';
}

interface NetworkPermissionTestData {
    type: 'networkPermissionTest';
}

interface DeriveEntropyData {
    type: 'deriveEntropy';
}

interface BatchedPermissionTestData {
    type: 'batchedPermissionTest';
}

interface WildcardPermissionTestData {
    type: 'wildcardPermissionTest';
}

interface ChainSubmitPermissionTestData {
    type: 'chainSubmitPermissionTest';
}

interface PaymentBalanceData {
    type: 'paymentBalance';
}

interface PaymentRequestData {
    type: 'paymentRequest';
}

interface PaymentTrackingData {
    type: 'paymentTracking';
    /** Hex of the 32-byte payment id, so the message survives as plain JSON. */
    paymentIdHex: string;
}

interface PaymentTopUpData {
    type: 'paymentTopUp';
}

interface RfcAllowanceData {
    type: 'rfcAllowance';
}

interface StatementSubscribeData {
    type: 'statementSubscribe';
}

interface UserIdentityData {
    type: 'userIdentity';
}

interface SignVrfData {
    type: 'signVrf';
}

interface NavigateData {
    type: 'navigate';
}

type MessageData = CounterData | BalanceData | TransferData | SignRawData | RingVrfData | CreateRoomFormData | NotificationTestData | NetworkPermissionTestData | DeriveEntropyData | BatchedPermissionTestData | WildcardPermissionTestData | ChainSubmitPermissionTestData | PaymentBalanceData | PaymentRequestData | PaymentTrackingData | PaymentTopUpData | RfcAllowanceData | StatementSubscribeData | UserIdentityData | SignVrfData | NavigateData;

// ============================================================================
// Host API & Chain Client
// ============================================================================

const HOST_PLAYGROUND_URL = 'https://host-playground.dot';

const POP_GENESIS_HASH = '0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48';
const ASSET_HUB_GENESIS_HASH = '0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a';
const ROOM_ID = 'default';

// Shared 32-byte topic so the statement subscription only matches statements this demo submits.
const DEMO_STATEMENT_TOPIC: Uint8Array = (() => {
    const topic = new Uint8Array(32);
    topic.set(new TextEncoder().encode('polkadotapp-demo-topic').slice(0, 32));
    return topic;
})();

const BALANCE_ASSET_ID = {
    parents: 1,
    interior: XcmV5Junctions.X3([
        XcmV5Junction.Parachain(1500),
        XcmV5Junction.PalletInstance(50),
        XcmV5Junction.GeneralIndex(50000413),
    ]),
};

const accountsProvider = createAccountsProvider();
const chat = createProductChatManager();

let popClient: ReturnType<typeof createClient> | null = null;
let popApi: ReturnType<ReturnType<typeof createClient>['getTypedApi']> | null = null;
let assetHubClient: ReturnType<typeof createClient> | null = null;
let assetHubApi: ReturnType<ReturnType<typeof createClient>['getTypedApi']> | null = null;
let chainProperties: { ss58Prefix: number; tokenDecimals: number; tokenSymbol: string } | null = null;
let assetMetadata: { decimals: number; symbol: string } | null = { decimals: 6, symbol: "CASH" };

// ============================================================================
// Bot Initialization (runs at module load — replaces onBotStarted global)
// ============================================================================

console.log('Products Demo: Bot started!');

const provider = createPapiProvider(POP_GENESIS_HASH);
popClient = createClient(provider);
popApi = popClient.getTypedApi(pop);

const assetHubProvider = createPapiProvider(ASSET_HUB_GENESIS_HASH);
assetHubClient = createClient(assetHubProvider);
assetHubApi = assetHubClient.getTypedApi(assetHub);

console.log('Products Demo: PAPI clients initialized (pop + assetHub)');

loadChainProperties().catch(e => console.log(`Products Demo: Failed to load chain properties: ${e}`));

chat.registerRoom({ roomId: ROOM_ID, name: 'Products Demo', icon: null }).then(status => {
    console.log(`Products Demo: Room registration status: ${status}`);
    if (status === 'New') {
        sendTextMessageViaChat("Hello! Send me a number to start a counter, or use /balance, /address, /transfer, /signraw, /ringvrf, /rooms, /notify, /network, /entropy, /batched, /wildcard, /chain, /paybalance, /payrequest, /paytopup, /rfc10, /userid, /signvrf, or /navigate");
    }
}).catch(e => console.log(`Products Demo: Failed to register room: ${e}`));

// ============================================================================
// Action Handling (replaces onUserMessage global)
// ============================================================================

chat.subscribeAction((action) => {
    console.log(`Bot received ${JSON.stringify(action)}`)

    switch (action.payload.tag) {
        case 'MessagePosted': {
            const msg = action.payload.value;
            if (msg.tag === 'Text') {
                onUserMessage(msg.value);
            }
            break;
        }
    }
});

// ============================================================================
// Bot Logic
// ============================================================================

async function loadChainProperties(): Promise<void> {
    const [chainSpec, ss58Prefix] = await Promise.all([
        popClient!.getChainSpecData(),
        popApi!.constants.System.SS58Prefix(),
    ]);

    const props = chainSpec.properties;
    const tokenDecimals = Array.isArray(props.tokenDecimals) ? props.tokenDecimals[0] : props.tokenDecimals;
    const tokenSymbol = Array.isArray(props.tokenSymbol) ? props.tokenSymbol[0] : props.tokenSymbol;

    chainProperties = { ss58Prefix, tokenDecimals, tokenSymbol };
    console.log(`Products Demo: Chain properties - ss58: ${ss58Prefix}, decimals: ${tokenDecimals}, symbol: ${tokenSymbol}`);
}

function onUserMessage(text: string): void {
    console.log(`Products Demo: User said "${text}"`);

    if (text.trim() === '/balance') {
        sendCustomMessage<BalanceData>({ type: 'balance' });
        return;
    }

    if (text.trim() === '/address') {
        resolveAddress().then(address => {
            sendTextMessageViaChat(address);
        }).catch(e => {
            console.log(`Products Demo: Failed to resolve address: ${e}`);
            sendTextMessageViaChat(`Error: ${e}`);
        });
        return;
    }

    if (text.trim() === '/transfer') {
        sendCustomMessage<TransferData>({ type: 'transfer' });
        return;
    }

    if (text.trim() === '/signraw') {
        sendCustomMessage<SignRawData>({ type: 'signRaw' });
        return;
    }

    if (text.trim() === '/ringvrf' || text.trim() === '/alias') {
        sendCustomMessage<RingVrfData>({ type: 'ringVrf' });
        return;
    }

    if (text.trim() === '/rooms') {
        sendCustomMessage<CreateRoomFormData>({ type: 'createRoomForm' });
        return;
    }

    if (text.trim() === '/notify') {
        sendCustomMessage<NotificationTestData>({ type: 'notificationTest' });
        return;
    }

    if (text.trim() === '/network') {
        sendCustomMessage<NetworkPermissionTestData>({ type: 'networkPermissionTest' });
        return;
    }

    if (text.trim() === '/entropy') {
        sendCustomMessage<DeriveEntropyData>({ type: 'deriveEntropy' });
        return;
    }

    if (text.trim() === '/batched') {
        sendCustomMessage<BatchedPermissionTestData>({ type: 'batchedPermissionTest' });
        return;
    }

    if (text.trim() === '/wildcard') {
        sendCustomMessage<WildcardPermissionTestData>({ type: 'wildcardPermissionTest' });
        return;
    }

    if (text.trim() === '/chain') {
        sendCustomMessage<ChainSubmitPermissionTestData>({ type: 'chainSubmitPermissionTest' });
        return;
    }

    if (text.trim() === '/paybalance') {
        sendCustomMessage<PaymentBalanceData>({ type: 'paymentBalance' });
        return;
    }

    if (text.trim() === '/payrequest') {
        sendCustomMessage<PaymentRequestData>({ type: 'paymentRequest' });
        return;
    }

    if (text.trim() === '/paytopup') {
        sendCustomMessage<PaymentTopUpData>({ type: 'paymentTopUp' });
        return;
    }

    if (text.trim() === '/rfc10') {
        sendCustomMessage<RfcAllowanceData>({ type: 'rfcAllowance' });
        return;
    }

    if (text.trim() === '/statements') {
        sendCustomMessage<StatementSubscribeData>({ type: 'statementSubscribe' });
        return;
    }

    if (text.trim() === '/userid') {
        sendCustomMessage<UserIdentityData>({ type: 'userIdentity' });
        return;
    }

    if (text.trim() === '/signvrf') {
        sendCustomMessage<SignVrfData>({ type: 'signVrf' });
        return;
    }

    if (text.trim() === '/navigate') {
        sendCustomMessage<NavigateData>({ type: 'navigate' });
        return;
    }

    // Parse number from text or default to 0
    const num = parseInt(text, 10) || 0;

    // Send a counter starting at that number
    const data: CounterData = { type: 'counter', initialCount: num };
    sendCustomMessage(data);
}

// ============================================================================
// SDK-based Message Sending
// ============================================================================

function sendTextMessageViaChat(text: string): void {
    chat.sendMessage(ROOM_ID, enumValue('Text', text));
}

function sendCustomMessage<T>(data: T): void {
    const payload = new TextEncoder().encode(JSON.stringify(data));
    chat.sendMessage(ROOM_ID, enumValue('Custom', {
        messageType: 'json',
        payload,
    }));
}

// ============================================================================
// React-based Renderer
// ============================================================================

chat.onCustomMessageRenderingRequest(
    registerChatMessageRenderer(
        (raw) => JSON.parse(new TextDecoder().decode(raw)) as MessageData,
        ({ messageType, payload: data }) => {
            console.log(`Products Demo: Rendering ${data.type} message`);

            switch (data.type) {
                case 'balance':
                    return <BalanceCard />;
                case 'transfer':
                    return <TransferCard />;
                case 'signRaw':
                    return <SignRawCard />;
                case 'ringVrf':
                    return <RingVrfCard />;
                case 'createRoomForm':
                    return <CreateRoomFormCard />;
                case 'notificationTest':
                    return <NotificationTestCard />;
                case 'networkPermissionTest':
                    return <NetworkPermissionTestCard />;
                case 'deriveEntropy':
                    return <DeriveEntropyCard />;
                case 'batchedPermissionTest':
                    return <BatchedPermissionTestCard />;
                case 'wildcardPermissionTest':
                    return <WildcardPermissionTestCard />;
                case 'chainSubmitPermissionTest':
                    return <ChainSubmitPermissionTestCard />;
                case 'paymentBalance':
                    return <PaymentBalanceCard />;
                case 'paymentRequest':
                    return <PaymentRequestCard />;
                case 'paymentTopUp':
                    return <PaymentTopUpCard />;
                case 'paymentTracking':
                    return <PaymentTrackingCard paymentIdHex={data.paymentIdHex} />;
                case 'rfcAllowance':
                    return <RfcAllowanceCard />;
                case 'statementSubscribe':
                    return <StatementSubscribeCard />;
                case 'userIdentity':
                    return <UserIdentityCard />;
                case 'signVrf':
                    return <SignVrfCard />;
                case 'navigate':
                    return <NavigateCard />;
                case 'counter':
                default:
                    const initialCount = data.initialCount ?? 0;
                    return <CounterCard initialCount={initialCount} />;
            }
        }
    )
);

// ============================================================================
// UI Components
// ============================================================================

function CounterCard({ initialCount }: { initialCount: number }) {
    const [count, setCount] = useState(initialCount);
    const [accountName, setAccountName] = useState<string | undefined>();

    useEffect(() => {
        Promise.resolve(accountsProvider.getProductAccount(OWN_PRODUCT_ID, 0)).then(result => {
            if (result.isOk()) {
                const publicKey = toHex(result.value.publicKey);
                setAccountName(publicKey);
                console.log(`accountGet success - publicKey: ${publicKey}`);
            } else {
                setAccountName(`Error: ${JSON.stringify(result.error)}`);
                console.log(`accountGet error: ${JSON.stringify(result.error)}`);
            }
        }).catch((e: unknown) => {
            console.log(`accountGet exception: ${e}`);
        });
    }, []);

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">
                {accountName != null ? `Account: ${accountName}` : 'Loading account...'}
            </Text>

            <Spacer height={12} />

            <Text style="headline.large" color="fg.primary">
                {`${count}`}
            </Text>

            <Spacer height={16} />

            <Row horizontalArrangement="center">
                <Button
                    text="-"
                    variant="secondary"
                    onClick={() => setCount(c => c - 1)}
                />

                <Spacer width={12} />

                <Button
                    text="+"
                    variant="primary"
                    onClick={() => setCount(c => c + 1)}
                />
            </Row>

            <Spacer height={12} />

            <Button
                text="Reset"
                variant="text"
                onClick={() => setCount(0)}
            />
        </Column>
    );
}

function BalanceCard() {
    const [balance, setBalance] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!popApi) {
            setError('Client not initialized');
            return;
        }

        let subscription: { unsubscribe(): void } | null = null;

        resolveAddress().then(address => {
            subscription = popApi!.query.Assets.Account.watchValue(BALANCE_ASSET_ID, address, { at: 'best' })
                .subscribe({
                    next: (emission: any) => {
                        const value = emission?.value;
                        setBalance(value != null ? formatBalance(value.balance) : '0');
                    },
                    error: (e: unknown) => {
                        setError(String(e));
                    },
                });
        }).catch((e: unknown) => setError(String(e)));

        return () => { subscription?.unsubscribe(); };
    }, []);

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Product account balance (on-chain)</Text>
            <Spacer height={4} />
            <Text style="body.small.regular" color="fg.tertiary">Queried directly from Assets pallet</Text>
            <Spacer height={12} />
            {error != null
                ? <Text style="body.large.regular" color="fg.primary">{`Error: ${error}`}</Text>
                : balance != null
                    ? <Text style="headline.large" color="fg.primary">{balance}</Text>
                    : <Text style="body.large.regular" color="fg.secondary">Loading...</Text>
            }
        </Column>
    );
}

function TransferCard() {
    const [status, setStatus] = useState<string>('Ready to transfer 1 UNIT to Alice');
    const [transferring, setTransferring] = useState(false);

    const handleTransfer = () => {
        if (!popApi || transferring) return;

        setTransferring(true);
        setStatus('Preparing transfer...');

        resolveProductAccountWithSigner().then(async ({ address, signer }) => {
            const decimals = chainProperties?.tokenDecimals ?? 10;
            const oneToken = 10n ** BigInt(decimals);

            const aliceAddress = getAliceAddress();

            setStatus('Awaiting signature...');

            const tx = popApi!.tx.Balances.transfer_keep_alive({
                dest: MultiAddress.Id(aliceAddress),
                value: oneToken,
            });

            tx.signSubmitAndWatch(signer).subscribe({
                next: (event: TxEvent) => {
                    console.log(`Products Demo: Transfer tx event: ${event.type}`);
                    if (event.type === 'txBestBlocksState') {
                        if (event.found) {
                            setStatus('Transfer successful!');
                        } else {
                            setStatus('Transaction pending...');
                        }
                        setTransferring(false);
                    }
                },
                error: (e: unknown) => {
                    console.log(`Products Demo: Transfer tx error: ${e}`);
                    setStatus(`Error: ${e}`);
                    setTransferring(false);
                },
            });
        }).catch((e: unknown) => {
            console.log(`Products Demo: Transfer failed: ${e}`);
            setStatus(`Error: ${e}`);
            setTransferring(false);
        });
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Transfer via Host API</Text>
            <Spacer height={8} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Button
                text="Send 1 UNIT to Alice"
                variant="primary"
                loading={transferring}
                onClick={handleTransfer}
            />
        </Column>
    );
}

function SignRawCard() {
    const [status, setStatus] = useState<string>('Ready');
    const [signature, setSignature] = useState<string | null>(null);
    const [signing, setSigning] = useState(false);

    const handleSignRaw = () => {
        if (signing) return;

        setSigning(true);
        setStatus('Awaiting signature...');

        resolveProductAccountWithSigner().then(async ({ signer }) => {
            const message = new TextEncoder().encode('Hello from Products Demo!');
            const result = await signer.signBytes(message);
            const hex = Array.from(new Uint8Array(result))
                .map(b => b.toString(16).padStart(2, '0'))
                .join('');
            setSignature(`0x${hex}`);
            setStatus('Signed successfully!');
            setSigning(false);
        }).catch((e: unknown) => {
            console.log(`Products Demo: Sign raw failed: ${e}`);
            setStatus(`Error: ${e}`);
            setSigning(false);
        });
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Sign Raw via Host API</Text>
            <Spacer height={8} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            {signature != null && (
                <>
                    <Spacer height={8} />
                    <Text style="body.small.regular" color="fg.secondary">
                        {`${signature.slice(0, 24)}...${signature.slice(-8)}`}
                    </Text>
                </>
            )}
            <Spacer height={16} />
            <Button
                text="Sign Message"
                variant="primary"
                loading={signing}
                onClick={handleSignRaw}
            />
        </Column>
    );
}

function toHex(bytes: Uint8Array): `0x${string}` {
    return `0x${Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')}`;
}

/**
 * The product's own dotNS id. Must match the id it is registered under, because the host derives a product
 * account from `//{productId}//{index}` — a mismatch silently hands out an account nothing else uses.
 */
const OWN_PRODUCT_ID = 'productsample.js.paseo';

// RFC-0004 amended by RFC-0022: context = (productId, suffix), where the suffix is the same
// selector an account carries — a plain index here. The ring addresses a Members-pallet ring on the PoP chain.
const PROOF_CONTEXT: ProofContext = [OWN_PRODUCT_ID, 0];
const MEMBERS_PALLET_INDEX = 67;

// RingCollectionId is a 32-byte, space-padded identifier (matches RingCollectionId.Companion.PEOPLE / PEOPLE_LITE).
function ringCollectionId(name: string): Uint8Array {
    const bytes = new Uint8Array(32).fill(0x20);
    bytes.set(new TextEncoder().encode(name).slice(0, 32));
    return bytes;
}

function peopleRing(collectionId: Uint8Array): CodecType<typeof RingLocation> {
    return {
        chainId: POP_GENESIS_HASH,
        junctions: [
            { tag: 'PalletInstance', value: MEMBERS_PALLET_INDEX },
            { tag: 'CollectionId', value: collectionId },
        ],
    };
}

const FULL_PEOPLE_RING = peopleRing(ringCollectionId('pop:polkadot.network/people'));
const LITE_PEOPLE_RING = peopleRing(ringCollectionId('pop:polkadot.network/people-lite'));

type RingKind = 'full' | 'lite';

/** The product account a top up draws on — the one "Copy product account id" hands out to fund. */
const TOP_UP_ACCOUNT_INDEX = 0;
const PERSONHOOD_PRODUCT_ID = 'peopl.dot';

function ringOf(kind: RingKind): CodecType<typeof RingLocation> {
    return kind === 'full' ? FULL_PEOPLE_RING : LITE_PEOPLE_RING;
}

function sameRing(a: CodecType<typeof RingLocation>, b: CodecType<typeof RingLocation>): boolean {
    return a.chainId === b.chainId &&
        a.junctions.length === b.junctions.length &&
        a.junctions.every((junction, i) => {
            const other = b.junctions[i];
            if (junction.tag !== other.tag) return false;
            return junction.tag === 'CollectionId'
                ? toHex(junction.value as Uint8Array) === toHex(other.value as Uint8Array)
                : junction.value === other.value;
        });
}

/** Renders a ring as chain + pallet + collection, so a near-miss is visible rather than just "no match". */
function describeRing(ring: CodecType<typeof RingLocation>): string {
    const pallet = ring.junctions.find(j => j.tag === 'PalletInstance');
    const collection = ring.junctions.find(j => j.tag === 'CollectionId');
    const collectionText = collection != null
        ? new TextDecoder().decode(collection.value as Uint8Array).trim()
        : '?';
    const chain = ring.chainId;
    return `${chain.slice(0, 10)}..${chain.slice(-4)} pallet ${pallet != null ? pallet.value : '?'} "${collectionText}"`;
}

function describeHandle(handle: RingVrfKeyHandle): string {
    const [owner, index] = handle;
    const rendered = typeof index === 'object' && 'value' in index ? String(index.value) : String(index);
    return `${owner} / ${rendered}`;
}

function short(bytes: Uint8Array): string {
    const hex = toHex(bytes);
    return `${hex.slice(0, 12)}...${hex.slice(-6)}`;
}

/**
 * RFC-0024: every ring VRF host call in one place. The key is named by an explicit handle - the host
 * no longer infers one from the ring - so the handle in use is what every call below depends on.
 */
function RingVrfCard() {
    const [index, setIndex] = useState('0');
    const [message, setMessage] = useState('product-sample-ring-vrf');
    const [ring, setRing] = useState<RingKind>('full');
    const [disclosure, setDisclosure] = useState<RingVrfKeyDisclosure>('Anonymized');
    const [foreignHandle, setForeignHandle] = useState<RingVrfKeyHandle | null>(null);
    const [status, setStatus] = useState('Press a button to call the host');
    const [busy, setBusy] = useState<string | null>(null);

    // Own keys are named directly; a foreign key is only ever the handle its owner listed, never a
    // handle we built - the index is the owner's implementation detail (RFC-0024).
    const activeHandle = foreignHandle ?? ringVrfKeyHandle(OWN_PRODUCT_ID, parseInt(index, 10) || 0);

    const run = (
        label: string,
        call: () => Promise<any>,
        describe: (value: any) => string,
    ) => {
        if (busy != null) return;

        setBusy(label);
        setStatus(`${label}...`);

        Promise.resolve(call()).then(result => {
            if (result.isOk()) {
                setStatus(`${label}: ${describe(result.value)}`);
                console.log(`Products Demo: ${label} success`);
            } else {
                setStatus(`${label} failed: ${JSON.stringify(result.error)}`);
                console.log(`Products Demo: ${label} error: ${JSON.stringify(result.error)}`);
            }
        }).catch((e: unknown) => {
            setStatus(`${label} threw: ${e}`);
            console.log(`Products Demo: ${label} exception: ${e}`);
        }).finally(() => {
            setBusy(null);
        });
    };

    const handleRegister = () => run(
        'registerRingVrfKey',
        () => Promise.resolve(accountsProvider.registerRingVrfKey(parseInt(index, 10) || 0, ringOf(ring))),
        publicKey => `member key ${short(publicKey)} for the ${ring} people ring`,
    );

    const handleListOwn = () => run(
        'listRingVrfKeys (own)',
        () => Promise.resolve(accountsProvider.listRingVrfKeys(OWN_PRODUCT_ID, disclosure)),
        (entries: RegisteredRingVrfKey[]) => entries.length === 0
            ? 'no keys registered yet'
            : entries.map(entry =>
                `${describeHandle(entry.handle)} [${entry.rings.length} ring(s)]${entry.publicKey != null ? ` key ${short(entry.publicKey)}` : ''}`
            ).join(' | '),
    );

    // Selecting by declared ring is the one rule a consuming product has to remember: hardcoding
    // ['peopl.dot', 0] breaks the moment the owner rotates or adds a key.
    const handleListPersonhood = () => run(
        'listRingVrfKeys (peopl.dot)',
        () => Promise.resolve(accountsProvider.listRingVrfKeys(PERSONHOOD_PRODUCT_ID, disclosure)),
        (entries: RegisteredRingVrfKey[]) => {
            const match = entries.find(entry => entry.rings.some((declared: CodecType<typeof RingLocation>) => sameRing(declared, ringOf(ring))));
            if (match == null) {
                setForeignHandle(null);
                const declared = entries.flatMap(entry => entry.rings).map(describeRing).join(' ; ');
                return `${entries.length} entr(ies), none matching. want [${describeRing(ringOf(ring))}] got [${declared}]`;
            }
            setForeignHandle(match.handle);
            return `using ${describeHandle(match.handle)} - selected by declared ring, not by index`;
        },
    );

    const handleUseOwnKey = () => {
        setForeignHandle(null);
        setStatus(`Active handle: ${describeHandle(ringVrfKeyHandle(OWN_PRODUCT_ID, parseInt(index, 10) || 0))}`);
    };

    const handleGetAlias = () => run(
        'getContextualAlias',
        () => Promise.resolve(accountsProvider.getContextualAlias(activeHandle, PROOF_CONTEXT, ringOf(ring))),
        value => `alias ${short(value.alias)} in context ${short(value.context)}`,
    );

    const handleCreateProof = () => run(
        'createRingVRFProof',
        () => Promise.resolve(accountsProvider.createRingVRFProof(
            activeHandle,
            PROOF_CONTEXT,
            ringOf(ring),
            new TextEncoder().encode(message),
        )),
        value => `proof ${short(value.proof)} · alias ${short(value.contextualAlias.alias)} · ring ${value.ringIndex} rev ${value.ringRevision}`,
    );

    const handleRingVrfSign = () => run(
        'ringVrfSign',
        () => Promise.resolve(accountsProvider.ringVrfSign(activeHandle, new TextEncoder().encode(message))),
        signature => `signature ${short(signature)} - linkable, verified against the member public key`,
    );

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Ring VRF via Host API (RFC-0024)</Text>
            <Spacer height={4} />
            <Text style="body.small.regular" color="fg.secondary">{`Handle: ${describeHandle(activeHandle)} (${foreignHandle != null ? 'foreign' : 'own'})`}</Text>

            <Spacer height={12} />

            <Row>
                <Button text="Full People" variant={ring === 'full' ? 'primary' : 'secondary'} onClick={() => setRing('full')} />
                <Spacer width={8} />
                <Button text="Lite People" variant={ring === 'lite' ? 'primary' : 'secondary'} onClick={() => setRing('lite')} />
            </Row>

            <Spacer height={8} />

            <Row>
                <Button text="Anonymized" variant={disclosure === 'Anonymized' ? 'primary' : 'secondary'} onClick={() => setDisclosure('Anonymized')} />
                <Spacer width={8} />
                <Button text="PublicKey" variant={disclosure === 'PublicKey' ? 'primary' : 'secondary'} onClick={() => setDisclosure('PublicKey')} />
            </Row>

            <Spacer height={12} />

            <TextField value={index} placeholder="Own key index" onValueChange={setIndex} />
            <Spacer height={8} />
            <TextField value={message} placeholder="Message to prove / sign" onValueChange={setMessage} />

            <Spacer height={16} />

            <Button text={`Register key for ${ring === 'full' ? 'Full' : 'Lite'} People`} variant="primary" loading={busy === 'registerRingVrfKey'} onClick={handleRegister} />
            <Spacer height={8} />
            <Button text="List own keys" variant="primary" loading={busy === 'listRingVrfKeys (own)'} onClick={handleListOwn} />
            <Spacer height={8} />
            <Button text="List peopl.dot keys" variant="primary" loading={busy === 'listRingVrfKeys (peopl.dot)'} onClick={handleListPersonhood} />
            <Spacer height={8} />
            <Button text="Use own key" variant="secondary" enabled={foreignHandle != null} onClick={handleUseOwnKey} />
            <Spacer height={8} />
            <Button text="Get alias" variant="primary" loading={busy === 'getContextualAlias'} onClick={handleGetAlias} />
            <Spacer height={8} />
            <Button text="Create proof" variant="primary" loading={busy === 'createRingVRFProof'} onClick={handleCreateProof} />
            <Spacer height={8} />
            <Button text="Ring VRF sign" variant="primary" loading={busy === 'ringVrfSign'} onClick={handleRingVrfSign} />

            <Spacer height={16} />

            <Text style="body.large.regular" color="fg.primary">{status}</Text>
        </Column>
    );
}

// RFC-0023 sr25519 VRF. Reproduces the People Chain airdrop transcript so the demo exercises the
// exact shape the runtime verifies: root label "pop:airdrop", then `domain` and `signer` appends.
const AIRDROP_TRANSCRIPT_LABEL = new TextEncoder().encode('pop:airdrop');

// "pop:game:airdrop:" right-padded with spaces to 28 bytes ++ big-endian u32 game index = 32 bytes.
function airdropEventId(gameIndex: number): Uint8Array {
    const eventId = new Uint8Array(32);
    eventId.set(new TextEncoder().encode('pop:game:airdrop:           '));
    new DataView(eventId.buffer).setUint32(28, gameIndex, false);
    return eventId;
}

function concatBytes(a: Uint8Array, b: Uint8Array): Uint8Array {
    const out = new Uint8Array(a.length + b.length);
    out.set(a);
    out.set(b, a.length);
    return out;
}

function SignVrfCard() {
    const [gameIndex, setGameIndex] = useState('7');
    const [preOutput, setPreOutput] = useState<string | null>(null);
    const [proof, setProof] = useState<string | null>(null);
    const [signer, setSigner] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);

    const handleSignVrf = () => {
        if (loading) return;

        setLoading(true);
        setError(null);
        setPreOutput(null);
        setProof(null);

        const index = parseInt(gameIndex, 10) || 0;

        // RFC-0023: the host never injects `signer` — the product fetches its own public key and
        // puts it into the transcript itself.
        Promise.resolve(accountsProvider.getProductAccount(OWN_PRODUCT_ID, 0)).then(accountResult => {
            if (!accountResult.isOk()) {
                setError(`accountGet error: ${JSON.stringify(accountResult.error)}`);
                setLoading(false);
                return;
            }

            const publicKey = accountResult.value.publicKey;
            setSigner(toHex(publicKey));

            const items = [
                { label: new TextEncoder().encode('domain'), value: concatBytes(AIRDROP_TRANSCRIPT_LABEL, airdropEventId(index)) },
                { label: new TextEncoder().encode('signer'), value: publicKey },
            ];

            return Promise.resolve(accountsProvider.signVrf(OWN_PRODUCT_ID, 0, AIRDROP_TRANSCRIPT_LABEL, items)).then(result => {
                if (result.isOk()) {
                    setPreOutput(toHex(result.value.preOutput));
                    setProof(toHex(result.value.proof));
                    console.log('Products Demo: signVrf success');
                } else {
                    setError(`signVrf error: ${JSON.stringify(result.error)}`);
                    console.log(`Products Demo: signVrf error: ${JSON.stringify(result.error)}`);
                }
            });
        }).catch((e: unknown) => {
            console.log(`Products Demo: signVrf exception: ${e}`);
            setError(`signVrf error: ${e}`);
        }).finally(() => {
            setLoading(false);
        });
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">sr25519 VRF via Host API (RFC-0023)</Text>
            <Spacer height={8} />
            <Text style="body.small.regular" color="fg.secondary">Transcript: pop:airdrop · domain + signer</Text>

            <Spacer height={12} />

            <TextField
                value={gameIndex}
                placeholder="Game index"
                onValueChange={setGameIndex}
            />

            <Spacer height={12} />

            {error != null && <Text style="body.large.regular" color="fg.primary">{error}</Text>}
            {signer != null && (
                <>
                    <Text style="body.small.regular" color="fg.secondary">Signer</Text>
                    <Spacer height={4} />
                    <Text style="body.large.regular" color="fg.primary">
                        {`${signer.slice(0, 24)}...${signer.slice(-8)}`}
                    </Text>
                </>
            )}
            {preOutput != null && (
                <>
                    <Spacer height={8} />
                    <Text style="body.small.regular" color="fg.secondary">Pre-output (32 bytes)</Text>
                    <Spacer height={4} />
                    <Text style="body.large.regular" color="fg.primary">
                        {`${preOutput.slice(0, 24)}...${preOutput.slice(-8)}`}
                    </Text>
                </>
            )}
            {proof != null && (
                <>
                    <Spacer height={8} />
                    <Text style="body.small.regular" color="fg.secondary">Proof (64 bytes)</Text>
                    <Spacer height={4} />
                    <Text style="body.large.regular" color="fg.primary">
                        {`${proof.slice(0, 24)}...${proof.slice(-8)}`}
                    </Text>
                </>
            )}
            {preOutput == null && error == null && (
                <Text style="body.large.regular" color="fg.secondary">Press the button to call the host</Text>
            )}

            <Spacer height={16} />

            <Button
                text="Sign VRF"
                variant="primary"
                loading={loading}
                onClick={handleSignVrf}
            />
        </Column>
    );
}

function CreateRoomFormCard() {
    const [roomName, setRoomName] = useState('');
    const [status, setStatus] = useState<string | null>(null);
    const [creating, setCreating] = useState(false);
    const [rooms, setRooms] = useState<{ roomId: string; participatingAs: string }[]>([]);

    useEffect(() => {
        const subscription = chat.subscribeChatList((updatedRooms) => {
            setRooms(updatedRooms);
        });

        return () => { subscription.unsubscribe(); };
    }, []);

    const handleCreateRoom = () => {
        const name = roomName.trim();
        if (!name || creating) return;

        setCreating(true);
        setStatus(null);

        const roomId = `user-room-${name.toLowerCase().replace(/\s+/g, '-')}`;

        chat.registerRoom({ roomId, name, icon: '' }).then(result => {
            setStatus(result === 'New' ? `Room "${name}" created!` : `Room "${name}" already exists`);

            if (result === 'New') {
                chat.sendMessage(roomId, enumValue('Text', `Welcome to ${name}!`));
            }

            setCreating(false);
        }).catch(e => {
            setStatus(`Error: ${e}`);
            setCreating(false);
        });
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Rooms</Text>
            <Spacer height={12} />

            {rooms.length === 0
                ? <Text style="body.large.regular" color="fg.secondary">No rooms yet</Text>
                : rooms.map((room) => (
                    <Column key={room.roomId}>
                        <Text style="body.large.regular" color="fg.primary">
                            {`${room.roomId} (${room.participatingAs})`}
                        </Text>
                        <Spacer height={4} />
                    </Column>
                ))
            }

            <Spacer height={16} />

            <TextField
                placeholder="Room name"
                value={roomName}
                onValueChange={setRoomName}
            />

            <Spacer height={12} />

            {status != null && (
                <>
                    <Text style="body.large.regular" color="fg.primary">{status}</Text>
                    <Spacer height={8} />
                </>
            )}

            <Button
                text="Create Room"
                variant="primary"
                loading={creating}
                onClick={handleCreateRoom}
            />
        </Column>
    );
}

async function resolveProductAccountWithSigner(): Promise<{ address: string; signer: PolkadotSigner }> {
    const accountResult = await accountsProvider.getProductAccount(OWN_PRODUCT_ID, 0);
    if (!accountResult.isOk()) throw new Error('Failed to get account');

    const account = {
        ...accountResult.value,
        dotNsIdentifier: OWN_PRODUCT_ID,
        derivationIndex: 0,
    };
    const ss58Prefix = chainProperties?.ss58Prefix ?? 42;
    const address = fromBufferToBase58(ss58Prefix)(account.publicKey);
    const signer = accountsProvider.getProductAccountSigner(account, 'createTransaction');

    return { address, signer };
}

function getAliceAddress(): string {
    const keyring = new Keyring({ type: 'sr25519' });
    const alicePair = keyring.addFromUri('//Alice');
    const ss58Prefix = chainProperties?.ss58Prefix ?? 42;
    return fromBufferToBase58(ss58Prefix)(alicePair.publicKey);
}

async function resolveAddress(): Promise<string> {
    const accountResult = await accountsProvider.getProductAccount(OWN_PRODUCT_ID, 0);
    if (!accountResult.isOk()) throw new Error('Failed to get account');

    const ss58Prefix = chainProperties?.ss58Prefix ?? 42;
    return fromBufferToBase58(ss58Prefix)(accountResult.value.publicKey);
}

function formatBalance(free: bigint): string {
    if (!assetMetadata) throw new Error('Asset metadata not loaded');
    const decimals = assetMetadata.decimals;
    const symbol = assetMetadata.symbol;
    const divisor = 10n ** BigInt(decimals);
    const whole = free / divisor;
    const fraction = (free % divisor).toString().padStart(decimals, '0').slice(0, 4);
    return `${whole}.${fraction} ${symbol}`;
}

// ============================================================================
// Notification Test Card
// ============================================================================

function NotificationTestCard() {
    const [seconds, setSeconds] = useState<string>('10');
    const [status, setStatus] = useState<string>('Not scheduled');
    const [scheduling, setScheduling] = useState(false);
    const [cancelling, setCancelling] = useState(false);
    const [lastScheduledId, setLastScheduledId] = useState<number | null>(null);

    async function handleSchedule() {
        const delaySeconds = parseInt(seconds, 10);
        if (Number.isNaN(delaySeconds) || delaySeconds < 0) {
            setStatus('Enter a non-negative number of seconds');
            return;
        }

        setScheduling(true);
        try {
            const permission = await requestDevicePermission('Notifications');
            if (permission.isErr() || !permission.value) {
                setStatus('Notification permission denied');
                return;
            }

            const scheduledAt = Date.now() + delaySeconds * 1000;
            const id = await notificationManager.push({
                text: `Scheduled reminder fired after ${delaySeconds}s`,
                scheduledAt,
            });
            setLastScheduledId(id);
            setStatus(`Scheduled (id ${id}) to fire in ${delaySeconds}s`);
        } catch (e: any) {
            setStatus(`Error: ${e?.message ?? e?.reason ?? String(e)}`);
        } finally {
            setScheduling(false);
        }
    }

    async function handleCancelLast() {
        if (lastScheduledId == null) return;

        setCancelling(true);
        try {
            await notificationManager.cancel(lastScheduledId);
            setStatus(`Cancelled notification id ${lastScheduledId}`);
            setLastScheduledId(null);
        } catch (e: any) {
            setStatus(`Error: ${e?.message ?? e?.reason ?? String(e)}`);
        } finally {
            setCancelling(false);
        }
    }

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Schedule Notification</Text>
            <Spacer height={12} />
            <TextField
                placeholder="Number of seconds to schedule notification in the future"
                value={seconds}
                onValueChange={setSeconds}
            />
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Button
                text="Schedule Notification"
                variant="primary"
                loading={scheduling}
                onClick={handleSchedule}
            />
            {lastScheduledId != null && (
                <>
                    <Spacer height={8} />
                    <Button
                        text="Cancel last"
                        variant="secondary"
                        loading={cancelling}
                        onClick={handleCancelLast}
                    />
                </>
            )}
        </Column>
    );
}

// ============================================================================
// Network Permission Test Card
// ============================================================================

const TEST_URL = 'https://jsonplaceholder.typicode.com/todos/1';

function NetworkPermissionTestCard() {
    const [status, setStatus] = useState<string>('Not requested');
    const [requesting, setRequesting] = useState(false);
    const [fetching, setFetching] = useState(false);

    async function handleRequestPermission() {
        setRequesting(true);
        try {
            const result = await hostApi.permission({
                tag: 'v1',
                value: [{ tag: 'Remote', value: ['jsonplaceholder.typicode.com'] }],
            });
            if (result.isOk()) {
                setStatus(result.value.value ? 'Permission granted' : 'Permission denied');
            } else {
                const err = result.error as any;
                setStatus(`Error: ${err?.value?.reason ?? JSON.stringify(err)}`);
            }
        } catch (e: any) {
            setStatus(`Error: ${e?.message ?? String(e)}`);
        } finally {
            setRequesting(false);
        }
    }

    async function handleFetch() {
        setFetching(true);
        try {
            const response = await fetch(TEST_URL);
            const data = await response.json();
            setStatus(`Fetched: ${JSON.stringify(data).slice(0, 80)}`);
        } catch (e: any) {
            setStatus(`Fetch error: ${e?.message ?? String(e)}`);
        } finally {
            setFetching(false);
        }
    }

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Network Permission Test</Text>
            <Spacer height={8} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Row>
                <Button
                    text="Request Permission"
                    variant="primary"
                    loading={requesting}
                    onClick={handleRequestPermission}
                />
                <Spacer width={8} />
                <Button
                    text="Fetch URL"
                    variant="secondary"
                    loading={fetching}
                    onClick={handleFetch}
                />
            </Row>
        </Column>
    );
}

// ============================================================================
// Derive Entropy Card
// ============================================================================

function DeriveEntropyCard() {
    const [key, setKey] = useState('my-app-key');
    const [entropy, setEntropy] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [deriving, setDeriving] = useState(false);

    const handleDerive = () => {
        if (deriving) return;

        setDeriving(true);
        setEntropy(null);
        setError(null);

        const keyBytes = new TextEncoder().encode(key);

        deriveEntropy(keyBytes).match(
            (value) => {
                setEntropy(toHex(value));
                setDeriving(false);
            },
            (err) => {
                setError(err.payload.reason);
                setDeriving(false);
            },
        );
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Derive Entropy via Host API</Text>
            <Spacer height={12} />

            <TextField
                placeholder="Derivation key"
                value={key}
                onValueChange={setKey}
            />

            <Spacer height={12} />

            {error != null
                ? <Text style="body.large.regular" color="fg.primary">{`Error: ${error}`}</Text>
                : entropy != null
                    ? (
                        <>
                            <Text style="body.small.regular" color="fg.secondary">Entropy (32 bytes)</Text>
                            <Spacer height={4} />
                            <Text style="body.large.regular" color="fg.primary">
                                {`${entropy.slice(0, 24)}...${entropy.slice(-8)}`}
                            </Text>
                        </>
                    )
                    : <Text style="body.large.regular" color="fg.secondary">Press button to derive</Text>
            }

            <Spacer height={16} />

            <Button
                text="Derive Entropy"
                variant="primary"
                loading={deriving}
                onClick={handleDerive}
            />
        </Column>
    );
}

// ============================================================================
// PANS-2391 Permission Model Test Cards
// ============================================================================

function BatchedPermissionTestCard() {
    const [status, setStatus] = useState<string>('Not requested');
    const [requesting, setRequesting] = useState(false);

    async function handleRequest() {
        setRequesting(true);
        try {
            const result = await hostApi.permission({
                tag: 'v1',
                value: [
                    { tag: 'Remote', value: ['api.example.com', 'cdn.example.com'] },
                    { tag: 'WebRtc', value: undefined },
                    { tag: 'ChainSubmit', value: undefined },
                    { tag: 'StatementSubmit', value: undefined },
                ],
            });
            if (result.isOk()) {
                setStatus(result.value.value ? 'Batch granted (single decision)' : 'Batch denied');
            } else {
                const err = result.error as any;
                setStatus(`Error: ${err?.value?.reason ?? JSON.stringify(err)}`);
            }
        } catch (e: any) {
            setStatus(`Error: ${e?.message ?? String(e)}`);
        } finally {
            setRequesting(false);
        }
    }

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Batched Permission Test</Text>
            <Spacer height={4} />
            <Text style="body.large.regular" color="fg.secondary">Remote (2) + WebRTC + ChainSubmit + StatementSubmit</Text>
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Button
                text="Request all 4 variants"
                variant="primary"
                loading={requesting}
                onClick={handleRequest}
            />
        </Column>
    );
}

function WildcardPermissionTestCard() {
    const [status, setStatus] = useState<string>('Not requested');
    const [grantingParent, setGrantingParent] = useState(false);
    const [checkingChild, setCheckingChild] = useState(false);

    async function handleGrantParent() {
        setGrantingParent(true);
        try {
            const result = await hostApi.permission({
                tag: 'v1',
                value: [{ tag: 'Remote', value: ['*.example.com'] }],
            });
            if (result.isOk()) {
                setStatus(result.value.value ? 'Granted *.example.com' : 'Denied *.example.com');
            } else {
                const err = result.error as any;
                setStatus(`Error: ${err?.value?.reason ?? JSON.stringify(err)}`);
            }
        } finally {
            setGrantingParent(false);
        }
    }

    async function handleCheckChild() {
        setCheckingChild(true);
        try {
            const result = await hostApi.permission({
                tag: 'v1',
                value: [{ tag: 'Remote', value: ['a.example.com'] }],
            });
            if (result.isOk()) {
                setStatus(result.value.value ? 'a.example.com: true (no prompt expected)' : 'a.example.com: false');
            } else {
                const err = result.error as any;
                setStatus(`Error: ${err?.value?.reason ?? JSON.stringify(err)}`);
            }
        } finally {
            setCheckingChild(false);
        }
    }

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Wildcard Domain Test</Text>
            <Spacer height={4} />
            <Text style="body.large.regular" color="fg.secondary">1. Grant *.example.com  2. Check a.example.com (should skip prompt)</Text>
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Button
                text="Grant *.example.com"
                variant="primary"
                loading={grantingParent}
                onClick={handleGrantParent}
            />
            <Spacer height={8} />
            <Button
                text="Check a.example.com"
                variant="secondary"
                loading={checkingChild}
                onClick={handleCheckChild}
            />
        </Column>
    );
}

function ChainSubmitPermissionTestCard() {
    const [status, setStatus] = useState<string>('Not requested');
    const [requesting, setRequesting] = useState(false);

    async function handleRequest() {
        setRequesting(true);
        try {
            const result = await hostApi.permission({
                tag: 'v1',
                value: [{ tag: 'ChainSubmit', value: undefined }],
            });
            if (result.isOk()) {
                setStatus(result.value.value ? 'ChainSubmit granted' : 'ChainSubmit denied');
            } else {
                const err = result.error as any;
                setStatus(`Error: ${err?.value?.reason ?? JSON.stringify(err)}`);
            }
        } catch (e: any) {
            setStatus(`Error: ${e?.message ?? String(e)}`);
        } finally {
            setRequesting(false);
        }
    }

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">ChainSubmit Permission Test</Text>
            <Spacer height={4} />
            <Text style="body.large.regular" color="fg.secondary">Standalone ChainSubmit via remote_permission</Text>
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Button
                text="Request ChainSubmit"
                variant="primary"
                loading={requesting}
                onClick={handleRequest}
            />
        </Column>
    );
}

// ============================================================================
// Payment Balance Card (RFC-0006 host_payment_balance_subscribe)
// ============================================================================

function PaymentBalanceCard() {
    const [available, setAvailable] = useState<bigint | null>(null);
    const [status, setStatus] = useState<string>('Requesting permission...');

    useEffect(() => {
        const subscription = paymentManager.subscribeBalance((balance) => {
            setAvailable(balance.available);
            setStatus('Live');
        });

        subscription.onInterrupt(() => {
            setStatus('Subscription interrupted (permission denied or host error)');
        });

        return () => { subscription.unsubscribe(); };
    }, []);

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">User's spendable balance (via host)</Text>
            <Spacer height={4} />
            <Text style="body.small.regular" color="fg.tertiary">Reported by host_payment_balance_subscribe</Text>
            <Spacer height={8} />
            {available != null
                ? <Text style="headline.large" color="fg.primary">{formatBalance(available)}</Text>
                : <Text style="body.large.regular" color="fg.secondary">{status}</Text>
            }
            {available != null && (
                <>
                    <Spacer height={4} />
                    <Text style="body.small.regular" color="fg.secondary">{status}</Text>
                </>
            )}
        </Column>
    );
}

// ============================================================================
// Statement-Store Subscription Card
// ============================================================================

function StatementSubscribeCard() {
    const [count, setCount] = useState<number>(0);
    const [latest, setLatest] = useState<SignedStatement | null>(null);
    const [status, setStatus] = useState<string>('Subscribing…');

    useEffect(() => {
        const store = createStatementStore();
        const subscription = store.subscribe({ matchAll: [DEMO_STATEMENT_TOPIC] }, (page) => {
            setCount(page.statements.length);
            setLatest(page.statements[page.statements.length - 1] ?? null);
            setStatus(`Live — ${page.statements.length} statement(s), complete: ${page.isComplete}`);
        });

        subscription.onInterrupt(() => {
            setStatus('Subscription interrupted (permission denied or host error)');
        });

        return () => { subscription.unsubscribe(); };
    }, []);

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Statement-store subscription (demo topic)</Text>
            <Spacer height={4} />
            <Text style="body.small.regular" color="fg.tertiary">Reported by statementStoreSubscribe</Text>
            <Spacer height={8} />
            <Text style="headline.large" color="fg.primary">{count}</Text>
            <Spacer height={4} />
            <Text style="body.small.regular" color="fg.secondary">{status}</Text>
            {latest?.data && (
                <>
                    <Spacer height={4} />
                    <Text style="body.small.regular" color="fg.tertiary">latest data: {toHex(latest.data).slice(0, 34)}…</Text>
                </>
            )}
        </Column>
    );
}

// ============================================================================
// Payment Request Card (RFC-0006 host_payment_request)
// ============================================================================

function decimalToPlanks(decimal: string, precision: number): bigint {
    const trimmed = decimal.trim();
    if (!/^\d+(\.\d+)?$/.test(trimmed)) {
        throw new Error(`Invalid decimal amount: "${decimal}"`);
    }
    const [whole, fraction = ''] = trimmed.split('.');
    const padded = (fraction + '0'.repeat(precision)).slice(0, precision);
    return BigInt(whole + padded);
}

function PaymentRequestCard() {
    const [idText, setIdText] = useState('payment-1');
    const [amountText, setAmountText] = useState('0.1');
    const [destinationText, setDestinationText] = useState('');
    const [status, setStatus] = useState<string>('Ready');
    const [busy, setBusy] = useState(false);
    const watchRef = useRef<{ unsubscribe: () => void } | null>(null);

    useEffect(() => () => watchRef.current?.unsubscribe(), []);

    const parsedId = (): Uint8Array | null => {
        try {
            return paymentIdFromLabel(idText);
        } catch (e) {
            setStatus(describeError(e));
            return null;
        }
    };

    const handleUseOwnAccount = () => {
        if (busy) return;
        setBusy(true);
        setStatus('Resolving product account…');

        Promise.resolve(accountsProvider.getProductAccount(OWN_PRODUCT_ID, TOP_UP_ACCOUNT_INDEX)).then(result => {
            if (result.isErr()) {
                setStatus(`Could not resolve the product account: ${describeError(result.error)}`);
                return;
            }

            setDestinationText(fromBufferToBase58(chainProperties?.ss58Prefix ?? 42)(result.value.publicKey));
            setStatus('Destination set to the product account');
        }).catch((e: unknown) => {
            setStatus(`Could not resolve the product account: ${describeError(e)}`);
        }).finally(() => setBusy(false));
    };

    const handleRequest = () => {
        if (busy) return;

        const id = parsedId();
        if (!id) return;

        const destination = getSs58AddressInfo(destinationText.trim());
        if (!destination.isValid) {
            setStatus('Destination must be a valid SS58 address');
            return;
        }

        const decimals = assetMetadata?.decimals;
        if (decimals == null) {
            setStatus('Asset metadata not loaded yet');
            return;
        }

        let planks: bigint;
        try {
            planks = decimalToPlanks(amountText, decimals);
        } catch (e) {
            setStatus(describeError(e));
            return;
        }

        setBusy(true);
        setStatus('Awaiting user approval…');

        // Resolves once the host has registered the payment, not once the funds arrive.
        paymentManager.requestPayment(planks, destination.publicKey, id)
            .then(() => {
                sendCustomMessage<PaymentTrackingData>({ type: 'paymentTracking', paymentIdHex: toHex(id) });
                setStatus('Registered ✅ — tracking message posted');
            })
            .catch((e: unknown) => setStatus(`Payment request failed: ${describeError(e)}`))
            .finally(() => setBusy(false));
    };

    const handleWatchStatus = () => {
        const id = parsedId();
        if (!id) return;

        watchRef.current?.unsubscribe();
        setStatus('Subscribing…');

        const decimals = assetMetadata?.decimals ?? 0;
        const symbol = assetMetadata?.symbol ?? '';

        const subscription = paymentManager.subscribePaymentStatus(id, (s) => {
            setStatus(describePaymentStatus(s, symbol, decimals));
        });
        subscription.onInterrupt((e: unknown) => setStatus(`Status unavailable: ${describeError(e)}`));

        watchRef.current = subscription;
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Payment Request (host)</Text>
            <Spacer height={12} />
            <TextField
                placeholder="Payment id"
                value={idText}
                onValueChange={setIdText}
            />
            <Spacer height={8} />
            <TextField
                placeholder="Amount"
                value={amountText}
                onValueChange={setAmountText}
            />
            <Spacer height={8} />
            <TextField
                placeholder="Destination (SS58)"
                value={destinationText}
                onValueChange={setDestinationText}
            />
            <Spacer height={16} />
            <Button
                text="Use own product account"
                variant="secondary"
                loading={busy}
                onClick={handleUseOwnAccount}
            />
            <Spacer height={8} />
            <Button
                text="Request payment"
                variant="primary"
                loading={busy}
                onClick={handleRequest}
            />
            <Spacer height={8} />
            <Button
                text="Watch status"
                variant="secondary"
                onClick={handleWatchStatus}
            />
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
        </Column>
    );
}

// ============================================================================
// Payment Top Up Card (RFC-0006 host_payment_top_up)
// ============================================================================

/** The host takes a 32-byte opaque id. A short label is padded so a human can type one and reuse it. */
function paymentIdFromLabel(label: string): Uint8Array {
    const bytes = new TextEncoder().encode(label.trim());
    if (bytes.length === 0) throw new Error('Id must not be empty');
    if (bytes.length > 32) throw new Error(`Id must be at most 32 bytes, got ${bytes.length}`);

    const id = new Uint8Array(32);
    id.set(bytes);
    return id;
}

/** Host errors arrive as codec enums (`{ tag }`), not `Error`s. */
function describeError(e: unknown): string {
    if (e && typeof e === 'object' && 'tag' in e) {
        const tag = (e as { tag: string }).tag;
        const reason = (e as { value?: { reason?: string } }).value?.reason;
        return reason ? `${tag}: ${reason}` : tag;
    }
    return e instanceof Error ? e.message : String(e);
}

function describeTopUpStatus(status: TopUpStatus, symbol: string, decimals: number): string {
    switch (status.type) {
        case 'detecting':
            return 'Detecting — waiting for the amount to appear at the source';
        case 'claiming':
            return 'Claiming — in progress';
        case 'claimed':
            return status.finalized ? 'Claimed ✅ (finalized)' : 'Claimed — awaiting finality';
        case 'claimedPartially':
            return `Claimed partially ⚠️ — ${planksToDecimal(status.actualClaimed, decimals)} ${symbol}`;
        case 'notClaimed':
            return 'Not claimed ❌ — the host never saw a balance at the source';
    }
}

function describePaymentStatus(status: PaymentStatus, symbol: string, decimals: number): string {
    switch (status.type) {
        case 'processing':
            return 'Processing on chain';
        case 'completed':
            return 'Completed ✅';
        case 'failed':
            return `Failed ❌ — ${status.reason}`;
        case 'partiallyClaimed':
            return `Partially claimed ⚠️ — ${planksToDecimal(status.actualClaimed, decimals)} ${symbol}`;
    }
}

function planksToDecimal(planks: bigint, decimals: number): string {
    const base = 10n ** BigInt(decimals);
    const whole = planks / base;
    const fraction = (planks % base).toString().padStart(decimals, '0').replace(/0+$/, '');
    return fraction ? `${whole}.${fraction}` : `${whole}`;
}

function PaymentTopUpCard() {
    const [idText, setIdText] = useState('topup-1');
    const [amountText, setAmountText] = useState('0.1');
    const [status, setStatus] = useState<string>('Ready');
    const [busy, setBusy] = useState(false);
    const watchRef = useRef<{ unsubscribe: () => void } | null>(null);

    // Only one watch at a time, and none once the card is gone: a subscription left open keeps reporting
    // into a card nobody is looking at.
    useEffect(() => () => watchRef.current?.unsubscribe(), []);

    const parsedId = (): Uint8Array | null => {
        try {
            return paymentIdFromLabel(idText);
        } catch (e) {
            setStatus(describeError(e));
            return null;
        }
    };

    const handleCopyAccountId = () => {
        if (busy) return;
        setBusy(true);
        setStatus('Resolving product account…');

        Promise.resolve(accountsProvider.getProductAccount(OWN_PRODUCT_ID, TOP_UP_ACCOUNT_INDEX)).then(result => {
            if (result.isErr()) {
                setStatus(`Could not resolve the product account: ${describeError(result.error)}`);
                return;
            }

            const address = fromBufferToBase58(chainProperties?.ss58Prefix ?? 42)(result.value.publicKey);

            // The renderer draws native widgets, so there is no document to copy from and no guarantee the
            // sandbox exposes a clipboard. The address is shown either way — that is what it is needed for.
            const clipboard = (globalThis as { navigator?: { clipboard?: { writeText(text: string): Promise<void> } } })
                .navigator?.clipboard;

            if (clipboard) {
                clipboard.writeText(address)
                    .then(() => setStatus(`Copied ✅ ${address}`))
                    .catch(() => setStatus(`Fund this account: ${address}`));
            } else {
                setStatus(`Fund this account: ${address}`);
            }
        }).catch((e: unknown) => {
            setStatus(`Could not resolve the product account: ${describeError(e)}`);
        }).finally(() => setBusy(false));
    };

    const handleStartTopUp = () => {
        if (busy) return;

        const id = parsedId();
        if (!id) return;

        const decimals = assetMetadata?.decimals;
        if (decimals == null) {
            setStatus('Asset metadata not loaded yet');
            return;
        }

        let planks: bigint;
        try {
            planks = decimalToPlanks(amountText, decimals);
        } catch (e) {
            setStatus(describeError(e));
            return;
        }

        setBusy(true);
        setStatus('Registering top up…');

        // Returns once the operation is registered, not once it is done — the outcome is what "Watch status"
        // is for. The host drives it to completion on its own, across restarts included.
        paymentManager.topUp(planks, { type: 'productAccount', derivationIndex: TOP_UP_ACCOUNT_INDEX }, id)
            .then(() => setStatus('Registered ✅ — use "Watch status" to follow it'))
            .catch((e: unknown) => setStatus(`Top up rejected: ${describeError(e)}`))
            .finally(() => setBusy(false));
    };

    const handleWatchStatus = () => {
        const id = parsedId();
        if (!id) return;

        watchRef.current?.unsubscribe();
        setStatus('Subscribing…');

        const decimals = assetMetadata?.decimals ?? 0;
        const symbol = assetMetadata?.symbol ?? '';

        const subscription = paymentManager.subscribeTopUpStatus(id, (s) => {
            setStatus(describeTopUpStatus(s, symbol, decimals));
        });
        subscription.onInterrupt((e: unknown) => setStatus(`Status unavailable: ${describeError(e)}`));

        watchRef.current = subscription;
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Payment Top Up (host)</Text>
            <Spacer height={12} />
            <TextField
                placeholder="Top up id"
                value={idText}
                onValueChange={setIdText}
            />
            <Spacer height={8} />
            <TextField
                placeholder="Amount"
                value={amountText}
                onValueChange={setAmountText}
            />
            <Spacer height={16} />
            <Button
                text="Copy product account id"
                variant="secondary"
                loading={busy}
                onClick={handleCopyAccountId}
            />
            <Spacer height={8} />
            <Button
                text="Start top up"
                variant="primary"
                loading={busy}
                onClick={handleStartTopUp}
            />
            <Spacer height={8} />
            <Button
                text="Watch status"
                variant="secondary"
                onClick={handleWatchStatus}
            />
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
        </Column>
    );
}

// ============================================================================
// Payment Tracking Card — one instance per initiated payment
// ============================================================================

function PaymentTrackingCard({ paymentIdHex }: { paymentIdHex: string }) {
    const [status, setStatus] = useState<string>('Subscribing...');

    useEffect(() => {
        const decimals = assetMetadata?.decimals ?? 0;
        const symbol = assetMetadata?.symbol ?? '';

        const subscription = paymentManager.subscribePaymentStatus(fromHex(paymentIdHex), (s) => {
            setStatus(describePaymentStatus(s, symbol, decimals));
        });
        subscription.onInterrupt((e: unknown) => setStatus(`Status unavailable: ${describeError(e)}`));

        return () => { subscription.unsubscribe(); };
    }, [paymentIdHex]);

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Payment</Text>
            <Spacer height={4} />
            <Text style="body.small.regular" color="fg.tertiary">{`id=${paymentIdHex.slice(0, 14)}…`}</Text>
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
        </Column>
    );
}

// ============================================================================
// RFC-0010: Allowance Management
// ============================================================================

function RfcAllowanceCard() {
    const [status, setStatus] = useState<string>('Idle');
    const [busy, setBusy] = useState(false);

    async function allocate(label: string, resources: { tag: string; value: any }[]) {
        if (busy) return;
        setBusy(true);
        setStatus(`Allocating ${label}…`);
        try {
            const result = await hostApi.requestResourceAllocation(enumValue('v1', resources as any));
            if (result.isOk() && result.value.tag === 'v1') {
                setStatus(`${label}: ${result.value.value.map(o => o.tag).join(', ')}`);
            } else {
                const errVal = result.isErr() ? (result.error as any) : 'unsupported version';
                setStatus(`${label} failed: ${errVal?.value?.reason ?? JSON.stringify(errVal)}`);
            }
        } catch (e) {
            setStatus(`${label} error: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setBusy(false);
        }
    }

    const allocateBulletin = () => allocate('Bulletin', [enumValue('BulletinAllowance', undefined)]);
    const allocateStatementStore = () => allocate('StatementStore', [enumValue('StatementStoreAllowance', undefined)]);
    const allocateSmartContract = () => allocate('SmartContract(0)', [enumValue('SmartContractAllowance', 0)]);
    const allocateAll = () => allocate('All resources', [
        enumValue('BulletinAllowance', undefined),
        enumValue('StatementStoreAllowance', undefined),
        enumValue('SmartContractAllowance', 0),
    ]);

    async function submitToBulletin() {
        if (busy) return;
        setBusy(true);
        setStatus('Submitting preimage to Bulletin…');
        try {
            const blob = new TextEncoder().encode(`hello-bulletin-${Date.now()}`);
            const hash = await preimageManager.submit(blob);
            setStatus(`Bulletin hash: ${hash.slice(0, 18)}…`);
        } catch (e) {
            setStatus(`Bulletin error: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setBusy(false);
        }
    }

    async function authorizeAndSubmitStatement() {
        if (busy) return;
        setBusy(true);
        setStatus('Creating Statement-Store proof…');
        try {
            const data = new TextEncoder().encode(`hello-sss-${Date.now()}`);
            const statementBody = {
                proof: undefined,
                decryptionKey: undefined,
                expiry: undefined,
                channel: undefined,
                topics: [DEMO_STATEMENT_TOPIC],
                data,
            };
            const proofResult = await hostApi.statementStoreCreateProofAuthorized(enumValue('v1', statementBody));
            if (!proofResult.isOk() || proofResult.value.tag !== 'v1') {
                const errVal = proofResult.isErr() ? (proofResult.error as any) : 'unsupported version';
                setStatus(`SSS proof failed: ${errVal?.value?.reason ?? JSON.stringify(errVal)}`);
                return;
            }
            const proof = proofResult.value.value;
            const signer = proof.tag === 'OnChain' ? proof.value.who : proof.value.signer;
            setStatus(`Proof ${proof.tag} signer ${toHex(signer).slice(0, 18)}…; submitting on-chain…`);

            const submitResult = await hostApi.statementStoreSubmit(enumValue('v1', { ...statementBody, proof }));
            if (submitResult.isOk() && submitResult.value.tag === 'v1') {
                setStatus(`Authorize + Submit ✅ (${proof.tag} ${toHex(signer).slice(0, 18)}…)`);
            } else {
                const errVal = submitResult.isErr() ? (submitResult.error as any) : 'unsupported version';
                setStatus(`Submit failed: ${errVal?.value?.reason ?? JSON.stringify(errVal)}`);
            }
        } catch (e) {
            setStatus(`SSS error: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setBusy(false);
        }
    }

    async function testPgasReviveCall() {
        if (busy) return;
        if (!assetHubApi) {
            setStatus('Asset Hub client not initialized');
            return;
        }
        setBusy(true);
        setStatus('Building Revive call on Asset Hub…');
        try {
            const { signer } = await resolveProductAccountWithSigner();

            // Arbitrary contract call — does not need to succeed. Triggers
            // SponsorReviveCallsWithPgas on the host (chainId == AH, module == Revive).
            const tx = assetHubApi.tx.Revive.call({
                dest: ('0x' + '00'.repeat(20)) as `0x${string}`,
                value: 0n,
                weight_limit: { ref_time: 1n, proof_size: 1n },
                storage_deposit_limit: 1n,
                data: new Uint8Array(0),
            });

            setStatus('Awaiting signature (PGAS sponsor will run)…');
            const customSignedExtensions = {
                AsPgas: { value: undefined },
                AsRingAlias: { value: undefined },
            };
            tx.signSubmitAndWatch(signer, { customSignedExtensions }).subscribe({
                next: (event: TxEvent) => {
                    console.log(`Products Demo: Revive tx event: ${event.type}`);
                    if (event.type === 'broadcasted') {
                        setStatus('Revive tx broadcasted ✅ (sponsor passed)');
                        setBusy(false);
                    } else if (event.type === 'txBestBlocksState' && event.found) {
                        setStatus('Revive tx in block (sponsor passed)');
                        setBusy(false);
                    }
                },
                error: (e: unknown) => {
                    // Failure is expected — we only care that signing reached the sponsor.
                    setStatus(`Revive tx error (likely on-chain): ${e instanceof Error ? e.message : String(e)}`);
                    setBusy(false);
                },
            });
        } catch (e) {
            setStatus(`PGAS test error: ${e instanceof Error ? e.message : String(e)}`);
            setBusy(false);
        }
    }

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">RFC-0010 Allowance Management</Text>
            <Spacer height={12} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Button text="Allocate Bulletin" variant="secondary" loading={busy} onClick={allocateBulletin} />
            <Spacer height={8} />
            <Button text="Allocate StatementStore" variant="secondary" loading={busy} onClick={allocateStatementStore} />
            <Spacer height={8} />
            <Button text="Allocate SmartContract(0)" variant="secondary" loading={busy} onClick={allocateSmartContract} />
            <Spacer height={8} />
            <Button text="Allocate All" variant="primary" loading={busy} onClick={allocateAll} />
            <Spacer height={16} />
            <Button text="Submit to Bulletin" variant="secondary" loading={busy} onClick={submitToBulletin} />
            <Spacer height={8} />
            <Button text="Authorize + Submit Statement" variant="secondary" loading={busy} onClick={authorizeAndSubmitStatement} />
            <Spacer height={8} />
            <Button text="Test PGAS (Revive on AH)" variant="secondary" loading={busy} onClick={testPgasReviveCall} />
        </Column>
    );
}

// ============================================================================
// User Identity Card (RFC-0014 host_get_user_id)
// ============================================================================

function UserIdentityCard() {
    const [username, setUsername] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);

    const handleGetUserId = () => {
        if (loading) return;

        setLoading(true);
        setError(null);
        setUsername(null);

        accountsProvider.getUserId().match(
            ({ primaryUsername }) => {
                setUsername(primaryUsername);
                console.log(`Products Demo: getUserId success - username: ${primaryUsername}`);
                setLoading(false);
            },
            (err) => {
                setError(`Error: ${err.name}`);
                console.log(`Products Demo: getUserId error: ${err.name}`);
                setLoading(false);
            },
        );
    };

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Get User ID (RFC-0014)</Text>
            <Spacer height={8} />
            {error != null
                ? <Text style="body.large.regular" color="fg.primary">{error}</Text>
                : username != null
                    ? (
                        <>
                            <Text style="body.small.regular" color="fg.secondary">Primary username</Text>
                            <Spacer height={4} />
                            <Text style="body.large.regular" color="fg.primary">{username}</Text>
                        </>
                    )
                    : <Text style="body.small.regular" color="fg.tertiary">Press button to fetch user ID</Text>
            }
            <Spacer height={16} />
            <Button
                text="Get User ID"
                variant="primary"
                loading={loading}
                onClick={handleGetUserId}
            />
        </Column>
    );
}

// ============================================================================
// Navigation Card (host_navigate_to)
// ============================================================================

function NavigateCard() {
    const [status, setStatus] = useState<string>('Press the button to leave the chat');
    const [navigating, setNavigating] = useState(false);

    async function handleNavigate() {
        if (navigating) return;

        setNavigating(true);
        try {
            const result = await hostApi.navigateTo({ tag: 'v1', value: HOST_PLAYGROUND_URL });
            if (result.isOk()) {
                setStatus(`Navigating to ${HOST_PLAYGROUND_URL}`);
                console.log('Products Demo: navigateTo success');
            } else {
                const err = result.error as any;
                setStatus(`Error: ${err?.value?.reason ?? JSON.stringify(err)}`);
                console.log(`Products Demo: navigateTo error: ${JSON.stringify(err)}`);
            }
        } catch (e: any) {
            setStatus(`Error: ${e?.message ?? String(e)}`);
        } finally {
            setNavigating(false);
        }
    }

    return (
        <Column
            padding={20}
            background={{ color: 'bg.surface.nested', shape: { tag: 'Rounded', value: 16 } }}
            horizontalAlignment="center"
        >
            <Text style="body.small.regular" color="fg.secondary">Navigate via Host API</Text>
            <Spacer height={8} />
            <Text style="body.large.regular" color="fg.primary">{status}</Text>
            <Spacer height={16} />
            <Button
                text="Open host-playground.dot"
                variant="primary"
                loading={navigating}
                onClick={handleNavigate}
            />
        </Column>
    );
}

console.log('products_demo.tsx loaded');
