// Breakpoint Profiler Overlay
// Displays FPS, frame time, and per-phase breakdown when WASM is compiled
// with the "profiling" feature. Toggle with F3. Hidden by default.
(function () {
    'use strict';

    const HISTORY_SIZE = 120;
    const frameTimes = [];
    let visible = false;
    let overlay = null;
    let lastScopes = [];

    // Phase colors for the breakdown bar
    const PHASE_COLORS = {
        frame:        '#666',
        network:      '#4fc3f7',
        overlay:      '#ab47bc',
        game_update:  '#66bb6a',
        camera:       '#ffa726',
        particles:    '#ef5350',
        weather:      '#78909c',
        render:       '#42a5f5',
        render_cull:  '#1565c0',
        render_batch: '#1976d2',
        render_draw:  '#1e88e5',
        render_postfx:'#7e57c2',
        postfx:       '#7e57c2',
        bridge:       '#ffca28',
        // Game-specific
        plat_physics:     '#c62828',
        plat_combat:      '#ad1457',
        plat_enemies:     '#6a1b9a',
        plat_damage:      '#283593',
        plat_powerups:    '#00838f',
        plat_rubber_band: '#2e7d32',
        plat_finish:      '#f9a825',
        golf_update:      '#66bb6a',
        lasertag_update:  '#42a5f5',
        tron_update:      '#ef5350',
    };

    function getColor(name) {
        return PHASE_COLORS[name] || '#888';
    }

    function createOverlay() {
        overlay = document.createElement('div');
        overlay.id = 'bp-profiler';
        overlay.innerHTML = `
            <div class="bp-prof-fps">-- FPS</div>
            <div class="bp-prof-frametime">-- ms</div>
            <div class="bp-prof-bar"></div>
            <div class="bp-prof-scopes"></div>
        `;
        document.body.appendChild(overlay);
    }

    function fpsColor(fps) {
        if (fps >= 55) return '#4caf50';
        if (fps >= 30) return '#ff9800';
        return '#f44336';
    }

    function updateOverlay() {
        if (!overlay || !visible) return;

        // FPS from frame time history
        const recent = frameTimes.slice(-60);
        const avgMs = recent.length > 0
            ? recent.reduce((a, b) => a + b, 0) / recent.length
            : 16.67;
        const fps = Math.round(1000 / avgMs);
        const minMs = recent.length > 0 ? Math.min(...recent) : 0;
        const maxMs = recent.length > 0 ? Math.max(...recent) : 0;

        const fpsEl = overlay.querySelector('.bp-prof-fps');
        fpsEl.textContent = `${fps} FPS`;
        fpsEl.style.color = fpsColor(fps);

        const ftEl = overlay.querySelector('.bp-prof-frametime');
        ftEl.textContent = `${avgMs.toFixed(1)}ms (${minMs.toFixed(1)}-${maxMs.toFixed(1)})`;

        // Phase breakdown bar (exclude "frame" as it's the total)
        const phases = lastScopes.filter(s => s.name !== 'frame');
        const totalUs = phases.reduce((sum, s) => sum + s.us, 0);
        const barEl = overlay.querySelector('.bp-prof-bar');
        if (totalUs > 0 && phases.length > 0) {
            barEl.innerHTML = phases.map(s => {
                const pct = (s.us / totalUs * 100).toFixed(1);
                const color = getColor(s.name);
                return `<div class="bp-prof-seg" style="width:${pct}%;background:${color}" title="${s.name}: ${(s.us/1000).toFixed(2)}ms (${pct}%)"></div>`;
            }).join('');
        } else {
            barEl.innerHTML = '';
        }

        // Top scopes list (sorted by time, show top 5)
        const sorted = [...phases].sort((a, b) => b.us - a.us).slice(0, 5);
        const scopesEl = overlay.querySelector('.bp-prof-scopes');
        scopesEl.innerHTML = sorted.map(s => {
            const color = getColor(s.name);
            const ms = (s.us / 1000).toFixed(2);
            return `<div class="bp-prof-scope"><span class="bp-prof-dot" style="background:${color}"></span>${s.name}: ${ms}ms</div>`;
        }).join('');
    }

    // Track frame times from rAF
    let lastFrameTime = 0;
    function trackFrame(timestamp) {
        if (lastFrameTime > 0) {
            const dt = timestamp - lastFrameTime;
            frameTimes.push(dt);
            if (frameTimes.length > HISTORY_SIZE) frameTimes.shift();
        }
        lastFrameTime = timestamp;
        if (visible) updateOverlay();
        requestAnimationFrame(trackFrame);
    }

    // Receive profile data from WASM
    window._breakpointProfileUpdate = function (data) {
        if (data && data.scopes) {
            lastScopes = data.scopes;
        }
    };

    // Toggle with F3
    document.addEventListener('keydown', function (e) {
        if (e.code === 'F3') {
            e.preventDefault();
            visible = !visible;
            if (!overlay) createOverlay();
            overlay.style.display = visible ? 'block' : 'none';
            if (visible) updateOverlay();
        }
    });

    requestAnimationFrame(trackFrame);
})();
