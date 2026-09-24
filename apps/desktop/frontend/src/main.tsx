import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { AppErrorBoundary } from './components/AppErrorBoundary';
import './styles.css';

async function start() {
  if (import.meta.env.VITE_WDIO_TEST === '1' && '__TAURI_INTERNALS__' in window) {
    await import('@wdio/tauri-plugin');
  }
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <AppErrorBoundary>
        <App />
      </AppErrorBoundary>
    </StrictMode>,
  );
}

void start();
