import type { Cyclone } from '../../contracts';
import { formatDecimal } from '../../lib/coordinates';
import styles from './inspector.module.css';

/** Forecast positions and geometry status remain readable without interpreting the map. */
export function CycloneForecast({ storm }: { readonly storm: Cyclone }) {
  const current = storm.geometryStatus === 'current';
  return (
    <section aria-label="Cyclone forecast">
      <h3 className={styles.subhead}>Forecast</h3>
      <p className={styles.historyNote}>NHC/CPHC coverage: Atlantic and eastern/central Pacific.</p>
      {!current ? (
        <p role="status">
          Forecast geometry {storm.geometryStatus}; the current position remains available.
        </p>
      ) : (
        <>
          <p className={styles.historyNote}>
            The cone shows uncertainty in the forecast cyclone centre.
          </p>
          {storm.forecastPoints.length === 0 ? (
            <p>No forecast positions reported.</p>
          ) : (
            <div className={styles.historyScroll}>
              <table className={styles.historyTable}>
                <caption>
                  Advisory {storm.geometryAdvisoryNumber ?? storm.advisoryNumber} forecast positions
                </caption>
                <thead>
                  <tr>
                    <th scope="col">Lead</th>
                    <th scope="col">Position</th>
                    <th scope="col">Wind</th>
                    <th scope="col">Gust</th>
                  </tr>
                </thead>
                <tbody>
                  {storm.forecastPoints.map((point) => (
                    <tr key={point.tauHours}>
                      <th scope="row">{point.tauHours} h</th>
                      <td>{formatDecimal(point.position)}</td>
                      <td>{point.windKt === undefined ? 'Not reported' : `${point.windKt} kn`}</td>
                      <td>{point.gustKt === undefined ? 'Not reported' : `${point.gustKt} kn`}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </section>
  );
}
