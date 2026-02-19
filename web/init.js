import init from './pkg/breakpoint_client.js';
try {
    await init();
    // WASM initialized — hide loading overlay
    const overlay = document.getElementById('loading-overlay');
    if (overlay) overlay.classList.add('hidden');
} catch (e) {
    console.error('Failed to initialize WASM:', e);
    const overlay = document.getElementById('loading-overlay');
    if (overlay) overlay.classList.add('hidden');
    const fatal = document.getElementById('fatal-error');
    const msg = document.getElementById('fatal-error-msg');
    if (fatal && msg) {
        msg.textContent = 'Failed to load the game engine. ' + (e.message || String(e));
        fatal.classList.remove('hidden');
    }
}
