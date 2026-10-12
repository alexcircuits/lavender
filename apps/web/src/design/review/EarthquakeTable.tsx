import { useMemo, useState } from 'react';
import { Panel } from '../primitives/Panel';
import { Table, type TableColumn, type TableSort } from '../primitives/Table';
import styles from './DesignReview.module.css';
import { type Quake, sampleQuakes } from './data';

const COLUMNS: ReadonlyArray<TableColumn<Quake>> = [
  {
    id: 'time',
    header: 'Time (UTC)',
    cell: (q) => q.time,
    mono: true,
    width: '184px',
    sortable: true,
  },
  {
    id: 'magnitude',
    header: 'Magnitude',
    cell: (q) => q.magnitude.toFixed(1),
    align: 'end',
    width: '96px',
    sortable: true,
  },
  { id: 'depth', header: 'Depth', cell: (q) => `${q.depthKm} km`, align: 'end', width: '80px' },
  { id: 'place', header: 'Place', cell: (q) => q.place },
  { id: 'id', header: 'Event ID', cell: (q) => q.id, mono: true, width: '112px' },
];

/** 1 000 rows: virtualised (only a window of rows is in the DOM). */
export function EarthquakeTable() {
  const quakes = useMemo(() => sampleQuakes(1000), []);
  const [sort, setSort] = useState<TableSort>({ columnId: 'time', direction: 'descending' });
  const rows = useMemo(() => {
    const sign = sort.direction === 'ascending' ? 1 : -1;
    const key = sort.columnId === 'magnitude' ? 'magnitude' : 'time';
    return [...quakes].sort((a, b) => (a[key] < b[key] ? -sign : a[key] > b[key] ? sign : 0));
  }, [quakes, sort]);

  return (
    <Panel
      title="Earthquakes"
      dense
      className={styles.tablePanel}
      toolbar={
        <p className={styles.footnote}>
          USGS · past 7 days · 1 000 events · sorted by {sort.columnId}
        </p>
      }
    >
      <Table
        aria-label="Earthquakes, past 7 days"
        columns={COLUMNS}
        rows={rows}
        getRowId={(q) => q.id}
        sort={sort}
        onSortChange={setSort}
        maxHeight="320px"
        minWidth="720px"
      />
    </Panel>
  );
}
