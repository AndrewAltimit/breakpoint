// Breakpoint UI — reads state from WASM via window._breakpointUpdate(state)
// and calls WASM exports via window._bpCreateRoom(), _bpJoinRoom(code), etc.

(function () {
    "use strict";

    // ── DOM refs ────────────────────────────────────────
    const $ = (id) => document.getElementById(id);

    const lobbyScreen    = $("lobby-screen");
    const gameHud        = $("game-hud");
    const betweenRounds  = $("between-rounds");
    const gameOver       = $("game-over");
    const playerNameInput = $("player-name");
    const joinCodeInput  = $("join-code");
    const lobbyStatus    = $("lobby-status");
    const lobbyError     = $("lobby-error");
    const roomInfo       = $("room-info");
    const roomCodeValue  = $("room-code-value");
    const playerList     = $("player-list");
    const btnCreate      = $("btn-create");
    const btnJoin        = $("btn-join");
    const btnStart       = $("btn-start");
    const btnMute        = $("btn-mute");
    const btnReturnLobby = $("btn-return-lobby");
    const hudGameName    = $("hud-game-name");
    const hudRound       = $("hud-round");
    const hudControls    = $("hud-controls");
    const roundScores    = $("round-scores");
    const roundInfoEl    = $("round-info");
    const finalScores    = $("final-scores");
    const tickerBar      = $("ticker-bar");
    const tickerText     = $("ticker-text");
    const toastContainer = $("toast-container");
    const btnDashboard   = $("btn-dashboard");
    const badgeCount     = $("badge-count");
    const disconnectBanner = $("disconnect-banner");

    // ── Game selector buttons ───────────────────────────
    const gameBtns = document.querySelectorAll(".game-btn");
    let selectedGame = "mini-golf";

    gameBtns.forEach((btn) => {
        btn.addEventListener("click", () => {
            gameBtns.forEach((b) => b.classList.remove("selected"));
            btn.classList.add("selected");
            selectedGame = btn.dataset.game;
            if (window._bpSelectGame) window._bpSelectGame(selectedGame);
        });
    });

    // ── Lobby actions ───────────────────────────────────
    btnCreate.addEventListener("click", () => {
        syncPlayerName();
        if (window._bpSelectGame) window._bpSelectGame(selectedGame);
        if (window._bpCreateRoom) window._bpCreateRoom();
    });

    btnJoin.addEventListener("click", () => {
        syncPlayerName();
        const code = joinCodeInput.value.trim().toUpperCase();
        if (!code) {
            lobbyError.textContent = "Enter a room code first";
            return;
        }
        if (window._bpJoinRoom) window._bpJoinRoom(code);
    });

    // Allow pressing Enter on join code input
    joinCodeInput.addEventListener("keydown", (e) => {
        if (e.key === "Enter") btnJoin.click();
    });

    btnStart.addEventListener("click", () => {
        if (window._bpStartGame) window._bpStartGame();
    });

    btnMute.addEventListener("click", () => {
        if (window._bpToggleMute) window._bpToggleMute();
    });

    btnReturnLobby.addEventListener("click", () => {
        if (window._bpReturnToLobby) window._bpReturnToLobby();
    });

    btnDashboard.addEventListener("click", () => {
        if (window._bpToggleDashboard) window._bpToggleDashboard();
    });

    // Sync player name input to WASM lobby state
    function syncPlayerName() {
        const name = playerNameInput.value.trim();
        if (name && window._bpSetPlayerName) {
            window._bpSetPlayerName(name);
        }
    }

    // ── Controls hints per game ─────────────────────────
    const CONTROLS = {
        "mini-golf": "Click to aim & shoot | Power = distance from ball",
        "platform-racer": "WASD / Arrows = Move | Space = Jump | E = Use Power-Up",
        "laser-tag": "WASD = Move | Mouse = Aim | Click = Fire | E = Power-Up",
        "tron": "A/D or Left/Right = Turn | Space = Brake",
    };

    // ── Game name display ───────────────────────────────
    const GAME_NAMES = {
        "mini-golf": "Mini Golf",
        "Golf": "Mini Golf",
        "platform-racer": "Platform Racer",
        "Platformer": "Platform Racer",
        "laser-tag": "Laser Tag",
        "LaserTag": "Laser Tag",
        "tron": "Tron",
        "Tron": "Tron",
    };

    // ── State update from WASM ──────────────────────────
    let prevState = null;

    window._breakpointUpdate = function (state) {
        updateScreens(state);
        updateLobby(state);
        updateHud(state);
        updateTronHud(state);
        updateScoreScreens(state);
        updateOverlay(state);
        updateMuteBtn(state);
        prevState = state;
    };

    window._breakpointDisconnect = function () {
        disconnectBanner.classList.remove("hidden");
    };

    window._breakpointReconnect = function () {
        disconnectBanner.classList.add("hidden");
    };

    // ── Screen visibility ───────────────────────────────
    function updateScreens(state) {
        const s = state.appState;

        lobbyScreen.classList.toggle("hidden", s !== "Lobby");
        gameHud.classList.toggle("hidden", s !== "InGame");
        betweenRounds.classList.toggle("hidden", s !== "BetweenRounds");
        gameOver.classList.toggle("hidden", s !== "GameOver");
    }

    // ── Lobby ───────────────────────────────────────────
    function updateLobby(state) {
        if (state.appState !== "Lobby") return;
        const lobby = state.lobby;

        // Sync player name display (only if user hasn't typed yet)
        if (!playerNameInput.value && lobby.playerName) {
            playerNameInput.value = lobby.playerName;
        }

        // Pre-fill join code from URL param
        if (!joinCodeInput.value && lobby.joinCodeInput) {
            joinCodeInput.value = lobby.joinCodeInput;
        }

        // Status/error messages
        lobbyStatus.textContent = lobby.statusMessage || "";
        lobbyError.textContent = lobby.errorMessage || "";

        // Room info visibility
        if (lobby.connected && lobby.roomCode) {
            roomInfo.classList.remove("hidden");
            roomCodeValue.textContent = lobby.roomCode;

            // Player list
            let html = "";
            for (const p of lobby.players) {
                html += `<div class="player-item">
                    <span>${escapeHtml(p.name)}</span>
                    ${p.isLeader ? '<span class="leader-badge">Leader</span>' : ""}
                </div>`;
            }
            playerList.innerHTML = html;

            // Start button (leader only)
            btnStart.classList.toggle("hidden", !lobby.isLeader);

            // Disable create/join after connected
            btnCreate.disabled = true;
            btnJoin.disabled = true;
            btnCreate.style.opacity = "0.4";
        } else {
            roomInfo.classList.add("hidden");
            btnCreate.disabled = false;
            btnJoin.disabled = false;
            btnCreate.style.opacity = "";
        }

        // Highlight selected game button
        const sel = lobby.selectedGame || selectedGame;
        gameBtns.forEach((btn) => {
            btn.classList.toggle("selected", btn.dataset.game === sel);
        });
    }

    // ── HUD ─────────────────────────────────────────────
    function updateHud(state) {
        if (state.appState !== "InGame") return;

        const gameId = state.game ? state.game.gameId : selectedGame;
        hudGameName.textContent = GAME_NAMES[gameId] || gameId || "";
        hudControls.textContent = CONTROLS[gameId] || CONTROLS[selectedGame] || "";

        if (state.roundTracker) {
            hudRound.textContent = `Round ${state.roundTracker.currentRound} / ${state.roundTracker.totalRounds}`;
            hudRound.classList.remove("hidden");
        } else {
            hudRound.classList.add("hidden");
        }
    }

    // ── Tron HUD (player names, minimap, gauges) ────────
    const tronHudContainer = $("tron-hud-container");
    const tronMinimap      = $("tron-minimap");
    const tronGauges       = $("tron-gauges");
    const tronSpeedFill    = $("tron-speed-fill");
    const tronRubberFill   = $("tron-rubber-fill");
    const tronBrakeFill    = $("tron-brake-fill");
    const tronMinimapCtx   = tronMinimap ? tronMinimap.getContext("2d") : null;
    let tronNameEls        = new Map();
    let tronMinimapFrame   = 0;

    const PLAYER_COLORS_CSS = [
        "#00d9ff", "#ffcc00", "#1aff33", "#ff0099",
        "#9933ff", "#ff5900", "#00ffb3", "#ff1a1a",
    ];

    function updateTronHud(state) {
        const hud = state.tronHud;
        if (!hud || !hud.players) {
            // Hide tron HUD elements
            if (tronHudContainer) tronHudContainer.innerHTML = "";
            if (tronMinimap) tronMinimap.classList.remove("visible");
            if (tronGauges) tronGauges.classList.add("hidden");
            tronNameEls.clear();
            return;
        }

        updateTronPlayerNames(hud.players);
        updateTronGauges(hud.players);

        // Minimap — update every 5th frame for performance
        tronMinimapFrame++;
        if (tronMinimapFrame % 5 === 0) {
            updateTronMinimap(hud);
        }
    }

    function updateTronPlayerNames(players) {
        if (!tronHudContainer) return;

        const currentIds = new Set();
        for (const p of players) {
            const key = p.name + p.color;
            currentIds.add(key);

            let el = tronNameEls.get(key);
            if (!el) {
                el = document.createElement("div");
                el.className = "tron-player-name";
                tronHudContainer.appendChild(el);
                tronNameEls.set(key, el);
            }

            el.textContent = p.name;
            el.style.color = p.color;
            el.style.left = p.screenX + "px";
            el.style.top = (p.screenY - 10) + "px";
            el.classList.toggle("dead", !p.alive);
            el.classList.toggle("local", p.isLocal);

            // Hide if offscreen
            const visible = p.screenX > -50 && p.screenX < window.innerWidth + 50
                         && p.screenY > -50 && p.screenY < window.innerHeight + 50;
            el.style.display = visible ? "" : "none";
        }

        // Remove stale labels
        for (const [key, el] of tronNameEls) {
            if (!currentIds.has(key)) {
                el.remove();
                tronNameEls.delete(key);
            }
        }
    }

    function updateTronGauges(players) {
        if (!tronGauges) return;

        const local = players.find((p) => p.isLocal);
        if (!local) {
            tronGauges.classList.add("hidden");
            return;
        }

        tronGauges.classList.remove("hidden");
        // Speed: 0-150 range
        const speedPct = Math.min(local.speed / 150, 1) * 100;
        tronSpeedFill.style.width = speedPct + "%";
        // Rubber: 0-1 range (consumed = 1 - rubber/max)
        const rubberPct = Math.min(local.rubber / 10, 1) * 100;
        tronRubberFill.style.width = rubberPct + "%";
        // Brake fuel: 0-1 range
        const brakePct = Math.min(local.brakeFuel / 5, 1) * 100;
        tronBrakeFill.style.width = brakePct + "%";
    }

    function updateTronMinimap(hud) {
        if (!tronMinimapCtx || !tronMinimap) return;

        tronMinimap.classList.add("visible");
        const ctx = tronMinimapCtx;
        const w = tronMinimap.width;
        const h = tronMinimap.height;
        const aw = hud.arenaWidth || 1;
        const ad = hud.arenaDepth || 1;

        ctx.clearRect(0, 0, w, h);

        // Arena border
        ctx.strokeStyle = "rgba(255,255,255,0.2)";
        ctx.lineWidth = 1;
        ctx.strokeRect(1, 1, w - 2, h - 2);

        // Scale helper
        const sx = (x) => (x / aw) * w;
        const sy = (z) => (z / ad) * h;

        // Draw wall segments
        if (hud.minimapWalls) {
            ctx.lineWidth = 1;
            for (const seg of hud.minimapWalls) {
                ctx.strokeStyle = PLAYER_COLORS_CSS[seg[4]] || "#fff";
                ctx.globalAlpha = 0.6;
                ctx.beginPath();
                ctx.moveTo(sx(seg[0]), sy(seg[1]));
                ctx.lineTo(sx(seg[2]), sy(seg[3]));
                ctx.stroke();
            }
            ctx.globalAlpha = 1.0;
        }

        // Draw cycles as bright dots
        if (hud.minimapCycles) {
            for (const cyc of hud.minimapCycles) {
                if (!cyc[3]) continue; // skip dead
                const color = PLAYER_COLORS_CSS[cyc[2]] || "#fff";
                ctx.fillStyle = color;
                ctx.shadowColor = color;
                ctx.shadowBlur = 4;
                ctx.beginPath();
                ctx.arc(sx(cyc[0]), sy(cyc[1]), 3, 0, Math.PI * 2);
                ctx.fill();
            }
            ctx.shadowBlur = 0;
        }
    }

    // ── Score screens ───────────────────────────────────
    function updateScoreScreens(state) {
        if (state.appState === "BetweenRounds" && state.roundTracker) {
            renderScores(roundScores, state.roundTracker.scores, state.lobby.players);
            roundInfoEl.textContent = `Round ${state.roundTracker.currentRound} of ${state.roundTracker.totalRounds}`;
        }

        if (state.appState === "GameOver" && state.roundTracker) {
            renderScores(finalScores, state.roundTracker.scores, state.lobby.players);
        }
    }

    function renderScores(container, scores, players) {
        if (!scores) {
            container.innerHTML = "<p>Waiting for scores...</p>";
            return;
        }

        // Convert scores object to sorted array
        const entries = Object.entries(scores)
            .map(([pid, score]) => ({
                pid: parseInt(pid),
                score,
                name: findPlayerName(parseInt(pid), players),
            }))
            .sort((a, b) => b.score - a.score);

        let html = "";
        entries.forEach((e, i) => {
            html += `<div class="score-row">
                <span class="rank">${i + 1}.</span>
                <span class="name">${escapeHtml(e.name)}</span>
                <span class="score">${e.score}</span>
            </div>`;
        });
        container.innerHTML = html;
    }

    function findPlayerName(pid, players) {
        if (!players) return `Player ${pid}`;
        const p = players.find((p) => p.id === pid);
        return p ? p.name : `Player ${pid}`;
    }

    // ── Overlay (ticker, toasts, badge) ─────────────────
    function updateOverlay(state) {
        const ov = state.overlay;
        if (!ov) return;

        // Ticker
        if (ov.tickerText) {
            tickerBar.classList.remove("hidden");
            tickerText.textContent = ov.tickerText;
        } else {
            tickerBar.classList.add("hidden");
        }

        // Badge
        if (ov.unreadCount > 0) {
            btnDashboard.classList.remove("hidden");
            badgeCount.classList.remove("hidden");
            badgeCount.textContent = ov.unreadCount;
        } else if (ov.pendingActions > 0) {
            btnDashboard.classList.remove("hidden");
            badgeCount.classList.add("hidden");
        } else {
            btnDashboard.classList.add("hidden");
        }

        // Toasts
        updateToasts(ov.toasts);
    }

    const activeToasts = new Map();

    function updateToasts(toasts) {
        if (!toasts) return;

        const currentIds = new Set(toasts.map((t) => t.id));

        // Remove dismissed toasts
        for (const [id, el] of activeToasts) {
            if (!currentIds.has(id)) {
                el.remove();
                activeToasts.delete(id);
            }
        }

        // Add/update toasts
        for (const toast of toasts) {
            if (activeToasts.has(toast.id)) {
                // Update claim status
                const el = activeToasts.get(toast.id);
                const actions = el.querySelector(".toast-actions");
                if (toast.claimedBy && actions) {
                    actions.innerHTML = `<span class="toast-claimed">Claimed by ${escapeHtml(toast.claimedBy)}</span>`;
                }
            } else {
                // Create new toast
                const el = document.createElement("div");
                el.className = `toast priority-${toast.priority}`;
                el.innerHTML = `
                    <div class="toast-title">${escapeHtml(toast.title)}</div>
                    <div class="toast-meta">${escapeHtml(toast.source || "")} ${toast.actor ? "by " + escapeHtml(toast.actor) : ""}</div>
                    <div class="toast-actions">
                        ${toast.claimedBy
                            ? `<span class="toast-claimed">Claimed by ${escapeHtml(toast.claimedBy)}</span>`
                            : `<button class="toast-claim-btn" onclick="window._bpClaimAlert && window._bpClaimAlert('${escapeAttr(toast.id)}')">Claim</button>`
                        }
                    </div>`;
                toastContainer.appendChild(el);
                activeToasts.set(toast.id, el);
            }
        }
    }

    // ── Mute button ─────────────────────────────────────
    function updateMuteBtn(state) {
        if (state.muted) {
            btnMute.classList.add("muted");
            btnMute.innerHTML = "&#x1f507;";
        } else {
            btnMute.classList.remove("muted");
            btnMute.innerHTML = "&#x1f50a;";
        }
    }

    // ── Helpers ─────────────────────────────────────────
    function escapeHtml(str) {
        if (!str) return "";
        return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    }

    function escapeAttr(str) {
        if (!str) return "";
        return str.replace(/'/g, "\\'").replace(/\\/g, "\\\\");
    }
})();
