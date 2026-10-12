import {
  ButtonsIsland,
  ControlsIsland,
  EmptyIsland,
  ErrorIsland,
  FieldsIsland,
  LoadingIsland,
  OverlaysIsland,
  StatusIsland,
  ToastsIsland,
  WarningIsland,
} from './Catalogue';
import styles from './DesignReview.module.css';
import { EarthquakeTable } from './EarthquakeTable';
import { ContactsIsland, InspectorIsland } from './InspectorIsland';
import { LayersIsland } from './LayersIsland';

/**
 * Dev-only design review (open `/#design-review` on the dev server). Every primitive in every
 * state, on the dark ground, laid out as realistic Lavender islands. Never shipped: App only
 * imports it when `import.meta.env.DEV`.
 */
export default function DesignReview() {
  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1 className={styles.title}>Design review</h1>
        <p className={styles.lede}>
          Every primitive in every state, on the globe&apos;s ground. Tab through it: focus, menus,
          dialogs and lists are all keyboard operable.
        </p>
      </header>

      <section className={styles.section} aria-labelledby="review-surfaces">
        <h2 id="review-surfaces" className={styles.sectionTitle}>
          Islands over the globe
        </h2>
        <div className={styles.flow}>
          <LayersIsland />
          <InspectorIsland />
          <ContactsIsland />
        </div>
      </section>

      <section className={styles.section} aria-labelledby="review-controls">
        <h2 id="review-controls" className={styles.sectionTitle}>
          Controls
        </h2>
        <div className={styles.flow}>
          <ButtonsIsland />
          <ControlsIsland />
          <FieldsIsland />
          <OverlaysIsland />
        </div>
      </section>

      <section className={styles.section} aria-labelledby="review-data">
        <h2 id="review-data" className={styles.sectionTitle}>
          Data and status
        </h2>
        <div className={styles.flow}>
          <StatusIsland />
          <ToastsIsland />
          <EarthquakeTable />
        </div>
      </section>

      <section className={styles.section} aria-labelledby="review-states">
        <h2 id="review-states" className={styles.sectionTitle}>
          Loading, empty and error
        </h2>
        <div className={styles.flow}>
          <LoadingIsland />
          <EmptyIsland />
          <ErrorIsland />
          <WarningIsland />
        </div>
      </section>
    </div>
  );
}
