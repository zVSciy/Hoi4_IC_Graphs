// Copies the graphify graph into the site as a trimmed snapshot.
//
//   1. rebuild the graph with graphify (repo root)
//   2. npm run sync-graph   (in site/)
//   3. commit site/public/graph.json and site/src/data/graph-meta.json
//
// graphify-out/ itself stays gitignored; only this snapshot is published.
import { readFileSync, writeFileSync, copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(site, '..', 'graphify-out', 'graph.json');
const graph = JSON.parse(readFileSync(source, 'utf8'));

const nodes = graph.nodes.map((n) => ({
  id: n.id,
  label: n.label ?? n.id,
  community: n.community ?? -1,
  community_name: n.community_name ?? `Community ${n.community}`,
  file: (n.source_file ?? '').replace(/\\/g, '/'),
  line: Number.parseInt(String(n.source_location ?? '').replace(/^L/, ''), 10) || null,
  type: n.file_type ?? '',
}));
const ids = new Set(nodes.map((n) => n.id));
const links = (graph.links ?? graph.edges)
  .filter((l) => ids.has(l.source) && ids.has(l.target))
  .map((l) => ({ source: l.source, target: l.target, relation: l.relation ?? '', confidence: l.confidence ?? '' }));

const commit = graph.built_at_commit ?? null;
writeFileSync(join(site, 'public', 'graph.json'), JSON.stringify({ commit, nodes, links }));

const communities = new Set(nodes.map((n) => n.community)).size;
const meta = { commit, nodes: nodes.length, links: links.length, communities };
mkdirSync(join(site, 'src', 'data'), { recursive: true });
writeFileSync(join(site, 'src', 'data', 'graph-meta.json'), JSON.stringify(meta, null, 2) + '\n');

// Screenshots used on the site come from the repo's docs.
const shots = join(site, '..', 'docs', 'screenshots');
mkdirSync(join(site, 'public', 'screenshots'), { recursive: true });
for (const f of ['main-window.png', 'graph-countries.png', 'graph-factions-share.png', 'rankings.png', 'game-library.png', 'settings.png']) {
  copyFileSync(join(shots, f), join(site, 'public', 'screenshots', f));
}

console.log(`graph snapshot: ${meta.nodes} nodes, ${meta.links} links, ${communities} communities, commit ${commit ?? '(none)'}`);
