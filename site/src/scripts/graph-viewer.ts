// 2D / 3D viewer for the graphify snapshot (public/graph.json).
//
// Both views are drawn by libraries from the same family (force-graph and
// 3d-force-graph), so switching only swaps the renderer: hidden communities,
// the path filter, the selected node and the search carry over.
import ForceGraph from 'force-graph';

type View = '2d' | '3d';

interface RawNode {
  id: string;
  label: string;
  community: number;
  community_name: string;
  file: string;
  line: number | null;
  type: string;
}
interface RawLink {
  source: string;
  target: string;
  relation: string;
  confidence: string;
}
interface Snapshot {
  commit: string | null;
  nodes: RawNode[];
  links: RawLink[];
}
// Objects handed to the renderer (it adds positions and resolves link ends).
type GNode = RawNode & { deg: number; x?: number; y?: number; z?: number };
type GLink = Omit<RawLink, 'source' | 'target'> & { source: string | GNode; target: string | GNode };

const REPO = 'https://github.com/zVSciy/Hoi4_IC_Graphs';

// Categorical palette built from GitHub's Primer dark scale hues.
const PALETTE = [
  '#58a6ff', '#3fb950', '#d29922', '#f778ba', '#a371f7', '#ff7b72', '#39c5cf', '#e3b341',
  '#7ee787', '#79c0ff', '#ffa657', '#d2a8ff', '#56d364', '#ff9bce', '#a5d6ff', '#f2cc60',
  '#bc8cff', '#ffa198', '#2ea043', '#db61a2', '#1f6feb', '#bb8009', '#8957e5', '#da3633',
  '#1b7c83', '#9e6a03', '#6e40c9', '#bf4b8a', '#388bfd', '#c69026', '#a475f9', '#f47067',
  '#56b6c2', '#e5c07b', '#98c379',
];
const color = (c: number) => (c < 0 ? '#8b949e' : PALETTE[c % PALETTE.length]);
const DIM = '#262c36';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const esc = (s: string) =>
  s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
const endId = (e: string | GNode) => (typeof e === 'string' ? e : e.id);
const nodeSize = (deg: number) => 1 + Math.sqrt(deg) * 1.6;

export async function startViewer() {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  const snap: Snapshot = await (await fetch(`${base}/graph.json`)).json();
  const commit = snap.commit ?? 'main';

  // ---------- static indexes ----------
  const degree = new Map<string, number>();
  const neighbours = new Map<string, { id: string; relation: string; out: boolean }[]>();
  for (const n of snap.nodes) neighbours.set(n.id, []);
  for (const l of snap.links) {
    degree.set(l.source, (degree.get(l.source) ?? 0) + 1);
    degree.set(l.target, (degree.get(l.target) ?? 0) + 1);
    neighbours.get(l.source)!.push({ id: l.target, relation: l.relation, out: true });
    neighbours.get(l.target)!.push({ id: l.source, relation: l.relation, out: false });
  }
  const rawById = new Map(snap.nodes.map((n) => [n.id, n]));
  const communities = new Map<number, string>();
  for (const n of snap.nodes) communities.set(n.community, n.community_name);

  // ---------- state (survives view switches) ----------
  const params = new URLSearchParams(location.search);
  const state = {
    view: (params.get('view') === '3d' ? '3d' : '2d') as View,
    hidden: new Set<number>(),
    files: params.get('files') ?? '',
    selected: null as string | null,
  };
  const onlyCommunity = params.get('community');
  if (onlyCommunity !== null && communities.has(Number(onlyCommunity))) {
    for (const c of communities.keys()) if (c !== Number(onlyCommunity)) state.hidden.add(c);
  }
  const startNode = params.get('node');
  if (startNode && rawById.has(startNode)) state.selected = startNode;

  const inFilter = (n: RawNode) => !state.files || n.file.startsWith(state.files);
  const visible = (n: RawNode) => inFilter(n) && !state.hidden.has(n.community);

  // ---------- renderer ----------
  const canvas = $('gv-canvas');
  let graph: any = null;
  let nodes: GNode[] = [];
  let links: GLink[] = [];
  let byId = new Map<string, GNode>();
  let selNbrs = new Set<string>();

  const isHot = (l: GLink) =>
    state.selected !== null && (endId(l.source) === state.selected || endId(l.target) === state.selected);
  const nodeColor = (n: GNode) =>
    state.selected && n.id !== state.selected && !selNbrs.has(n.id) ? DIM : color(n.community);
  const linkColor = (l: GLink) => {
    if (isHot(l)) return '#e6edf3';
    if (state.selected) return 'rgba(48,54,61,0.35)';
    if (l.confidence === 'INFERRED') return 'rgba(210,153,34,0.45)';
    if (l.confidence === 'AMBIGUOUS') return 'rgba(248,81,73,0.55)';
    return 'rgba(110,118,129,0.35)';
  };
  const tooltip = (n: GNode) =>
    `<b>${esc(n.label)}</b><br>${esc(n.community_name)}<br><small>${esc(n.file || 'no source file')}${n.line ? ':' + n.line : ''} · ${n.deg} links</small>`;

  // Runs `fn` once, when the layout settles (or after 3 s, whichever comes first).
  const onceSettled = (fn: () => void) => {
    let done = false;
    const run = () => {
      if (done) return;
      done = true;
      graph.onEngineStop(() => {});
      fn();
    };
    graph.onEngineStop(run);
    setTimeout(run, 3000);
  };

  function pushData() {
    const vis = nodes.filter(visible);
    const visIds = new Set(vis.map((n) => n.id));
    const vl = links.filter((l) => visIds.has(endId(l.source)) && visIds.has(endId(l.target)));
    graph.graphData({ nodes: vis, links: vl });
    $('gv-stats').textContent = `${vis.length} nodes · ${vl.length} links`;
  }

  function restyle() {
    selNbrs = new Set((state.selected && neighbours.get(state.selected)?.map((x) => x.id)) || []);
    graph
      .nodeColor(graph.nodeColor())
      .linkColor(graph.linkColor())
      .linkWidth(graph.linkWidth())
      .linkDirectionalParticles(graph.linkDirectionalParticles());
  }

  async function create(view: View) {
    if (graph) {
      graph.pauseAnimation?.();
      graph._destructor?.();
    }
    canvas.querySelectorAll(':scope > :not(.gv-loading)').forEach((el) => el.remove());
    const host = document.createElement('div');
    canvas.appendChild(host);

    // Fresh objects per renderer: each library stores its own positions on them.
    nodes = snap.nodes.map((n) => ({ ...n, deg: degree.get(n.id) ?? 0 }));
    links = snap.links.map((l) => ({ ...l }));
    byId = new Map(nodes.map((n) => [n.id, n]));

    const common = (g: any) =>
      g
        .width(canvas.clientWidth)
        .height(canvas.clientHeight)
        .backgroundColor('#0d1117')
        .nodeId('id')
        .nodeLabel(tooltip)
        .nodeVal((n: GNode) => nodeSize(n.deg))
        .nodeColor(nodeColor)
        .linkColor(linkColor)
        .linkWidth((l: GLink) => (isHot(l) ? 1.6 : 0.4))
        .linkDirectionalParticles((l: GLink) => (isHot(l) ? 2 : 0))
        .linkDirectionalParticleWidth(2)
        .linkDirectionalParticleColor(() => '#e6edf3')
        .onNodeClick((n: GNode) => select(n.id, true))
        .onBackgroundClick(() => select(null, false))
        .cooldownTicks(200);

    if (view === '2d') {
      graph = common(new ForceGraph(host))
        .nodeRelSize(4)
        .nodeCanvasObjectMode(() => 'after')
        .nodeCanvasObject((n: GNode, ctx: CanvasRenderingContext2D, scale: number) => {
          // Labels: always for the selection and its neighbours, otherwise only when zoomed in.
          const hot = n.id === state.selected || selNbrs.has(n.id);
          if (state.selected ? !hot : !(scale > 2.2 || (scale > 0.9 && n.deg >= 18))) return;
          const size = Math.max(11 / scale, 1.5);
          ctx.font = `${n.id === state.selected ? 600 : 400} ${size}px -apple-system, "Segoe UI", sans-serif`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'top';
          ctx.fillStyle = '#e6edf3';
          ctx.fillText(n.label, n.x!, n.y! + Math.sqrt(nodeSize(n.deg)) * 4 + 1);
        });
    } else {
      const { default: ForceGraph3D } = await import('3d-force-graph');
      graph = common(new ForceGraph3D(host, { controlType: 'orbit' }))
        .nodeRelSize(4)
        .nodeOpacity(0.95)
        .linkOpacity(0.5);
    }

    $('gv-loading').hidden = true;
    pushData();
    restyle();
    onceSettled(() => (state.selected ? focusOn(state.selected) : graph.zoomToFit(600, 40)));
  }

  function focusOn(id: string) {
    const n = byId.get(id);
    if (!n || n.x === undefined) return;
    if (state.view === '2d') {
      graph.centerAt(n.x, n.y, 700);
      graph.zoom(3, 700);
    } else {
      const len = Math.hypot(n.x, n.y ?? 0, n.z ?? 0) || 1;
      const r = 1 + 110 / len;
      graph.cameraPosition({ x: n.x * r, y: (n.y ?? 0) * r, z: (n.z ?? 0) * r }, n, 1000);
    }
  }

  // ---------- selection + details panel ----------
  const details = $('gv-details');

  function select(id: string | null, fly: boolean) {
    let reflowed = false;
    if (id) {
      const raw = rawById.get(id)!;
      // Make sure the node can actually be seen.
      if (!inFilter(raw)) {
        state.files = '';
        renderFilter();
      }
      state.hidden.delete(raw.community);
      if (!graph.graphData().nodes.includes(byId.get(id))) {
        pushData();
        reflowed = true;
      }
      renderLegend();
    }
    state.selected = id;
    restyle();
    renderDetails();
    syncUrl();
    if (id && fly) {
      if (reflowed) onceSettled(() => focusOn(id));
      else focusOn(id);
    }
  }

  function renderDetails() {
    const id = state.selected;
    if (!id) {
      details.hidden = true;
      return;
    }
    const n = rawById.get(id)!;
    const nb = (neighbours.get(id) ?? []).slice().sort((a, b) => a.relation.localeCompare(b.relation));
    const gh = n.file ? `${REPO}/blob/${commit}/${n.file}${n.line ? '#L' + n.line : ''}` : null;
    details.innerHTML = `
      <button type="button" class="btn btn-sm close" data-close aria-label="Close">✕</button>
      <h3>${esc(n.label)}</h3>
      <div class="row"><span class="label">${esc(n.type || 'node')}</span></div>
      <div class="row"><span class="sw" style="background:${color(n.community)}"></span> <b>${esc(n.community_name)}</b></div>
      <div class="row">${n.file ? `<code>${esc(n.file)}${n.line ? ':' + n.line : ''}</code>` : 'External or concept node (no source file)'}</div>
      ${gh ? `<p><a class="btn btn-sm" href="${gh}" target="_blank" rel="noopener">Open on GitHub ↗</a></p>` : ''}
      <div class="row"><b>${nb.length}</b> connections</div>
      <ul>${nb
        .slice(0, 80)
        .map((x) => {
          const t = rawById.get(x.id)!;
          return `<li><span class="rel">${x.out ? '→' : '←'} ${esc(x.relation)}</span>
            <button type="button" data-node="${esc(x.id)}" title="${esc(t.file)}">${esc(t.label)}</button></li>`;
        })
        .join('')}${nb.length > 80 ? `<li class="rel">… ${nb.length - 80} more</li>` : ''}</ul>`;
    details.hidden = false;
  }
  details.addEventListener('click', (e) => {
    const t = e.target as HTMLElement;
    if (t.closest('[data-close]')) select(null, false);
    const b = t.closest<HTMLElement>('[data-node]');
    if (b) select(b.dataset.node!, true);
  });

  // ---------- legend ----------
  const legend = $('gv-legend');
  function renderLegend() {
    const counts = new Map<number, number>();
    for (const n of snap.nodes) if (inFilter(n)) counts.set(n.community, (counts.get(n.community) ?? 0) + 1);
    const rows = [...communities.entries()].sort((a, b) => (counts.get(b[0]) ?? 0) - (counts.get(a[0]) ?? 0));
    legend.innerHTML = rows
      .map(([c, name]) => {
        const k = counts.get(c) ?? 0;
        const cls = [state.hidden.has(c) ? 'off' : '', k === 0 ? 'empty' : ''].join(' ');
        return `<li class="${cls}" data-c="${c}"><span class="sw" style="background:${color(c)}"></span><span>${esc(name)}</span><span class="cnt">${k}</span></li>`;
      })
      .join('');
  }
  function refreshAll() {
    if (state.selected && !visible(rawById.get(state.selected)!)) {
      state.selected = null;
      renderDetails();
    }
    renderLegend();
    pushData();
    restyle();
    syncUrl();
  }
  legend.addEventListener('click', (e) => {
    const li = (e.target as HTMLElement).closest<HTMLElement>('li[data-c]');
    if (!li) return;
    const c = Number(li.dataset.c);
    if (state.hidden.has(c)) state.hidden.delete(c);
    else state.hidden.add(c);
    refreshAll();
  });
  $('gv-all').onclick = () => {
    state.hidden.clear();
    refreshAll();
  };
  $('gv-none').onclick = () => {
    for (const c of communities.keys()) state.hidden.add(c);
    refreshAll();
  };

  // ---------- path filter banner ----------
  function renderFilter() {
    $('gv-filter').hidden = !state.files;
    $('gv-filter-path').textContent = state.files;
  }
  $('gv-filter-clear').onclick = () => {
    state.files = '';
    renderFilter();
    refreshAll();
    onceSettled(() => graph.zoomToFit(600, 40));
  };

  // ---------- search ----------
  const input = $<HTMLInputElement>('gv-search');
  const results = $('gv-results');
  let matches: RawNode[] = [];
  let active = 0;
  function renderResults() {
    const q = input.value.trim().toLowerCase();
    if (!q) {
      results.hidden = true;
      return;
    }
    const score = (n: RawNode) => {
      const l = n.label.toLowerCase();
      return l === q ? 0 : l.startsWith(q) ? 1 : l.includes(q) ? 2 : n.file.toLowerCase().includes(q) ? 3 : 9;
    };
    matches = snap.nodes
      .map((n) => [score(n), n] as const)
      .filter(([s]) => s < 9)
      .sort((a, b) => a[0] - b[0] || (degree.get(b[1].id) ?? 0) - (degree.get(a[1].id) ?? 0))
      .slice(0, 12)
      .map(([, n]) => n);
    active = 0;
    results.innerHTML = matches.length
      ? matches
          .map(
            (n, i) =>
              `<li class="${i === active ? 'active' : ''}" data-node="${esc(n.id)}"><span class="sw" style="background:${color(n.community)}"></span>${esc(n.label)}<small>${esc(n.file.split('/').pop() || '')}</small></li>`,
          )
          .join('')
      : '<li>No matches</li>';
    results.hidden = false;
  }
  input.addEventListener('input', renderResults);
  input.addEventListener('keydown', (e) => {
    if (results.hidden || !matches.length) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active + (e.key === 'ArrowDown' ? 1 : matches.length - 1)) % matches.length;
      results.querySelectorAll('li').forEach((li, i) => li.classList.toggle('active', i === active));
    } else if (e.key === 'Enter') {
      select(matches[active].id, true);
      results.hidden = true;
    } else if (e.key === 'Escape') {
      results.hidden = true;
    }
  });
  results.addEventListener('mousedown', (e) => {
    const li = (e.target as HTMLElement).closest<HTMLElement>('li[data-node]');
    if (li) {
      e.preventDefault();
      select(li.dataset.node!, true);
      results.hidden = true;
    }
  });
  input.addEventListener('blur', () => setTimeout(() => (results.hidden = true), 100));

  // ---------- toolbar ----------
  const viewButtons = document.querySelectorAll<HTMLButtonElement>('.segmented [data-view]');
  const setViewButtons = () =>
    viewButtons.forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.view === state.view)));
  viewButtons.forEach((b) =>
    b.addEventListener('click', async () => {
      const v = b.dataset.view as View;
      if (v === state.view) return;
      state.view = v;
      setViewButtons();
      syncUrl();
      $('gv-loading').textContent = v === '3d' ? 'Loading 3D renderer…' : 'Loading…';
      $('gv-loading').hidden = false;
      await create(v);
    }),
  );
  $('gv-reset').onclick = () => {
    select(null, false);
    graph.zoomToFit(600, 40);
  };

  function syncUrl() {
    const p = new URLSearchParams();
    if (state.view === '3d') p.set('view', '3d');
    if (state.files) p.set('files', state.files);
    if (state.selected) p.set('node', state.selected);
    const qs = p.toString();
    history.replaceState(null, '', location.pathname + (qs ? `?${qs}` : ''));
  }

  // The canvas resizes when the details panel opens or the window changes.
  new ResizeObserver(() => graph?.width(canvas.clientWidth).height(canvas.clientHeight)).observe(canvas);

  // ---------- go ----------
  setViewButtons();
  renderFilter();
  renderLegend();
  await create(state.view);
  renderDetails();
}
