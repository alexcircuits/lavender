import '../design/fonts.css';
import '../design/tokens.css';
import '../design/base.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { DesignProvider } from '../design';
import { DocsApp } from './DocsApp';

const element = document.getElementById('root');
if (!element) throw new Error('Lavender docs: #root is missing');
createRoot(element).render(
  <StrictMode>
    <DesignProvider>
      <DocsApp />
    </DesignProvider>
  </StrictMode>,
);
