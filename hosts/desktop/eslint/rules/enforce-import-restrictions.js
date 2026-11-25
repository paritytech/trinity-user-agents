import path from 'node:path';

import { resolve } from 'eslint-plugin-import-x/utils';
import { minimatch } from 'minimatch';

/**
 * Generic import/re-export deny-list, keyed by file location — not bound to any layer.
 *
 * Option shape (`eslint.config.js`):
 *
 *   {
 *     "./src/domains/*": {           // scope glob (a dir; `*` = one segment, `**` = any depth)
 *       "**\/gateway.ts": { forbid: ["**\/resource.ts", "**\/$usecase/*.ts"] },
 *       "**\/resource.ts": { forbid: ["**\/hooks.ts", "**\/$usecase/*.ts"] },
 *     },
 *   }
 *
 * For a linted file: its **scope root** is the shortest ancestor directory that
 * matches the scope glob (e.g. `./src/domains/*` -> `src/domains/<domain>`). The
 * file's path relative to that root is matched against the source patterns; for
 * every matching source pattern, the file may NOT import a target whose resolved
 * path — taken relative to the SAME scope root — matches one of the pattern's
 * `forbid` globs (union across matching patterns). Imports that resolve outside
 * the scope, or to `node_modules`, are ignored (only in-scope relative/aliased
 * imports are checked). A file matching no configured scope/pattern is
 * unrestricted. All matching is `minimatch`.
 *
 * Both `import ... from` and `export ... from` (named and `*`) are checked — a
 * re-export is an import that also widens the public surface, so it is the
 * stricter case, not an exempt one.
 *
 * Note: a barrel import (`import { fooUseCase } from '@/domains/x'`) resolves to
 * that domain's `index.ts`, not the use-case file — this rule matches by
 * resolved path, so it catches the relative/deep form (`../$usecase/foo`) but
 * not the by-symbol barrel form. Constrain the barrel's own `export ... from`
 * list instead: what a barrel cannot re-export, no consumer can reach by symbol.
 *
 * @type {import('eslint').Rule.RuleModule}
 */

const toPosix = p => p.replace(/\\/g, '/');
const stripDot = g => g.replace(/^\.\//, '');

// Shortest ancestor directory of `fileDir` (repo-relative, posix) that matches `scopeGlob`.
function scopeRootOf(fileDir, scopeGlob) {
  const parts = fileDir.split('/');
  for (let i = 1; i <= parts.length; i++) {
    const candidate = parts.slice(0, i).join('/');
    if (minimatch(candidate, scopeGlob)) return candidate;
  }
  return null;
}

export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Forbid imports of files matching a glob, by source file location (deny-list)',
      category: 'Layering',
      recommended: false,
    },
    schema: [
      {
        // scope glob -> { source pattern -> { forbid: [target globs] } }
        type: 'object',
        additionalProperties: {
          type: 'object',
          additionalProperties: {
            type: 'object',
            additionalProperties: false,
            properties: { forbid: { type: 'array', items: { type: 'string' } } },
            required: ['forbid'],
          },
        },
      },
    ],
    messages: {
      forbidden: '`{{source}}` may not import `{{target}}` (matches forbidden pattern `{{pattern}}`).',
      forbiddenReexport:
        '`{{source}}` may not re-export `{{target}}` (matches forbidden pattern `{{pattern}}`). Re-exporting puts it on the public surface, where any consumer can reach it by symbol.',
    },
  },
  create(context) {
    const config = context.options[0] ?? {};
    const cwd = context.cwd;
    const fileRel = toPosix(path.relative(cwd, path.resolve(context.filename)));
    if (fileRel.startsWith('..')) return {};
    const fileDir = path.posix.dirname(fileRel);

    // Resolve every configured scope this file falls in, with the union of the
    // `forbid` globs from its matching source patterns.
    const active = [];
    for (const [scopeGlobRaw, sources] of Object.entries(config)) {
      const scopeRoot = scopeRootOf(fileDir, stripDot(scopeGlobRaw));
      if (scopeRoot === null) continue;
      const inScope = path.posix.relative(scopeRoot, fileRel);
      const forbid = new Set();
      let matched = false;
      for (const [srcPattern, rule] of Object.entries(sources)) {
        if (minimatch(inScope, srcPattern)) {
          matched = true;
          for (const f of rule.forbid) forbid.add(f);
        }
      }
      if (matched) active.push({ scopeRoot, inScope, forbid: [...forbid] });
    }
    if (active.length === 0) return {};

    function check(node, messageId) {
      const spec = typeof node.source?.value === 'string' ? node.source.value : '';
      if (!spec) return;
      const resolved = resolve(spec, context);
      if (!resolved || resolved.includes('node_modules')) return;
      const resolvedRel = toPosix(path.relative(cwd, resolved));

      for (const { scopeRoot, inScope, forbid } of active) {
        const target = path.posix.relative(scopeRoot, resolvedRel);
        if (target.startsWith('..')) continue; // resolves outside this scope — ignored
        const pattern = forbid.find(p => minimatch(target, p));
        if (pattern) {
          context.report({ node, messageId, data: { source: inScope, target, pattern } });
        }
      }
    }

    return {
      ImportDeclaration: node => check(node, 'forbidden'),
      // `export { x } from './y'` / `export * from './y'` — no `source` on a local
      // `export { x }`, which `check` skips.
      ExportNamedDeclaration: node => check(node, 'forbiddenReexport'),
      ExportAllDeclaration: node => check(node, 'forbiddenReexport'),
    };
  },
};
