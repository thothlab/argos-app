/* @refresh reload */
import { render } from 'solid-js/web';

import App from './App';
import { installWorkspaceWatch } from './lib/workspace-watch';
import './index.css';

const root = document.getElementById('root');
if (!root) {
  throw new Error('Root element #root not found');
}

// Subscribe to file-watcher events from the Rust side. Best-effort —
// failures don't block the app boot.
installWorkspaceWatch().catch(() => {
  // Browser-mode or Tauri without the event plugin — running without a
  // watcher is fine, the user can manually reload via the welcome screen.
});

render(() => <App />, root);
