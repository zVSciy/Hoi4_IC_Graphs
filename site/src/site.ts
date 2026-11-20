import meta from './data/graph-meta.json';

export const REPO = 'https://github.com/zVSciy/Hoi4_IC_Graphs';
export const RELEASES = `${REPO}/releases/latest`;

/** The commit the graph snapshot (and every line link) refers to. */
export const COMMIT: string = meta.commit ?? 'main';
export const GRAPH_META = meta;
export const TREE_URL = `${REPO}/tree/${COMMIT}`;

/** Prefixes a site path with the configured base (`/Hoi4_IC_Graphs`). */
export function url(path = '/'): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  return base + (path.startsWith('/') ? path : `/${path}`);
}

/** Link to a file (and optionally a line) on GitHub at the snapshot commit. */
export function srcUrl(file: string, line?: number | null): string {
  return `${REPO}/blob/${COMMIT}/${file}${line ? `#L${line}` : ''}`;
}

export interface TourPage {
  slug: string;
  title: string;
  blurb: string;
}

export const TOUR: TourPage[] = [
  { slug: 'data', title: 'data', blurb: 'Parsing output.txt into a countries × days grid' },
  { slug: 'inject', title: 'inject', blurb: 'Finding HOI4 and loading the DataCollection DLL' },
  { slug: 'live', title: 'live', blurb: 'Following a file the game is still writing' },
  { slug: 'library', title: 'library', blurb: 'SQLite game library and its binary codec' },
  { slug: 'analysis', title: 'analysis', blurb: 'Datasets to daily and cumulative series, groups' },
  { slug: 'ui', title: 'ui', blurb: 'The egui front end and its windows' },
  { slug: 'export', title: 'export', blurb: 'HTML (Plotly), CSV and PNG export' },
  { slug: 'game-and-countries', title: 'game & countries', blurb: 'The HOI4 install, versions and country names' },
  { slug: 'support', title: 'support modules', blurb: 'main, settings, paths, theme, logging, update' },
  { slug: 'build-and-release', title: 'build & release', blurb: 'build.rs, CI and the release workflow' },
];
