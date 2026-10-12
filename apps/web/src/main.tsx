// Composition root: global styles → platform → runtime (stores, feed controller) → <App/>.
// The platform is chosen once here (ADR 0005); the globe is injected so the shell never
// imports the engine module directly.
import './design/fonts.css';
import './design/tokens.css';
import './design/base.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import { mountGlobe } from './globe';
import { createPlatform } from './platform';
import { createRuntime, startSelfCheck } from './runtime';

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Lavender: #root is missing from index.html');
const root = createRoot(rootElement);

createPlatform()
  .then((platform) => {
    const runtime = createRuntime(platform);
    runtime.start();
    if (platform.diagnostics) startSelfCheck(runtime, platform.diagnostics);
    root.render(
      <StrictMode>
        <App runtime={runtime} mountGlobe={mountGlobe} />
      </StrictMode>,
    );
  })
  .catch((error: unknown) => {
    // Only the desktop adapter can fail to load (a missing chunk); say so instead of a blank page.
    console.error('Lavender could not start its platform adapter', error);
    rootElement.textContent = 'Lavender could not start. Reload the page or reinstall the app.';
  });
