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
    const btnPlayAgain   = $("btn-play-again");
    const roundCountdown = $("round-countdown");
    const gameOverCountdown = $("game-over-countdown");
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
    const gameSettings   = $("game-settings");
    const settPlatformer = $("settings-platformer");
    const settLasertag   = $("settings-lasertag");
    let selectedGame = "mini-golf";

    // ── Game descriptions ───────────────────────────────
    const GAME_DESCS = {
        "mini-golf": "2-8 players \u00b7 Turn-based \u00b7 10 courses",
        "platform-racer": "2-6 players \u00b7 Race or Survive",
        "laser-tag": "2-8 players \u00b7 FFA or Teams",
        "tron": "2-8 players \u00b7 Light Cycles \u00b7 Bots available",
    };

    // Inject game descriptions into buttons
    gameBtns.forEach((btn) => {
        const desc = GAME_DESCS[btn.dataset.game];
        if (desc) {
            const descEl = document.createElement("div");
            descEl.className = "game-desc";
            descEl.textContent = desc;
            btn.appendChild(descEl);
        }
    });

    function updateGameSettingsPanel() {
        const panels = [settPlatformer, settLasertag];
        panels.forEach((p) => p && p.classList.add("hidden"));

        if (selectedGame === "platform-racer" && settPlatformer) {
            gameSettings.classList.remove("hidden");
            settPlatformer.classList.remove("hidden");
        } else if (selectedGame === "laser-tag" && settLasertag) {
            gameSettings.classList.remove("hidden");
            settLasertag.classList.remove("hidden");
        } else {
            gameSettings.classList.add("hidden");
        }
    }

    gameBtns.forEach((btn) => {
        btn.addEventListener("click", () => {
            gameBtns.forEach((b) => {
                b.classList.remove("selected");
                b.setAttribute("aria-pressed", "false");
            });
            btn.classList.add("selected");
            btn.setAttribute("aria-pressed", "true");
            selectedGame = btn.dataset.game;
            if (window._bpSelectGame) window._bpSelectGame(selectedGame);
            updateGameSettingsPanel();
        });
    });

    // ── Game settings change handlers ────────────────────
    function bindSettingSelect(id, key) {
        const el = $(id);
        if (!el) return;
        el.addEventListener("change", () => {
            if (window._bpSetGameSetting) {
                window._bpSetGameSetting(key, JSON.stringify(el.value));
            }
        });
    }

    bindSettingSelect("setting-platformer-mode", "mode");
    bindSettingSelect("setting-lasertag-team-mode", "team_mode");
    bindSettingSelect("setting-lasertag-arena-size", "arena_size");

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

    btnPlayAgain.addEventListener("click", () => {
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

    // ── ESC key handling ──────────────────────────────────
    document.addEventListener("keydown", (e) => {
        if (e.key !== "Escape") return;
        // Dismiss toasts on ESC during gameplay
        if (!gameHud.classList.contains("hidden")) {
            dismissAllToasts();
        }
    });

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
        updateGolfHud(state);
        updatePlatformerHud(state);
        updateLasertagHud(state);
        updateTronHud(state);
        updateScoreScreens(state);
        updateOverlay(state);
        updateMuteBtn(state);
        prevState = state;
    };

    // ── Fatal error display (called from Rust bridge) ────
    window._breakpointFatalError = function (msg) {
        const overlay = $("loading-overlay");
        if (overlay) overlay.classList.add("hidden");
        const fatal = $("fatal-error");
        const msgEl = $("fatal-error-msg");
        if (fatal && msgEl) {
            msgEl.textContent = msg;
            fatal.classList.remove("hidden");
        }
    };
    const btnFatalReload = $("btn-fatal-reload");
    if (btnFatalReload) btnFatalReload.addEventListener("click", () => location.reload());

    // ── Disconnect banner with attempt info ──────────────
    // Rust bridge passes a single JSON object: { attempt, maxAttempts, nextRetrySecs }
    window._breakpointDisconnect = function (info) {
        disconnectBanner.classList.remove("hidden");
        const attempt = info && info.attempt;
        const maxAttempts = info && info.maxAttempts;
        const nextRetrySecs = info && info.nextRetrySecs;
        if (typeof attempt === "number" && typeof maxAttempts === "number") {
            if (attempt >= maxAttempts) {
                disconnectBanner.textContent = "Connection lost. ";
                const rejoinBtn = document.createElement("button");
                rejoinBtn.id = "dc-rejoin";
                rejoinBtn.className = "dc-rejoin-btn";
                rejoinBtn.textContent = "Return to Lobby";
                rejoinBtn.addEventListener("click", () => {
                    if (window._bpReturnToLobby) window._bpReturnToLobby();
                    disconnectBanner.classList.add("hidden");
                });
                disconnectBanner.appendChild(rejoinBtn);
            } else {
                const retryText = typeof nextRetrySecs === "number" && nextRetrySecs > 0
                    ? `, retrying in ${Math.ceil(nextRetrySecs)}s`
                    : "";
                disconnectBanner.textContent =
                    `Connection lost. Reconnecting (attempt ${attempt + 1}/${maxAttempts}${retryText})...`;
            }
        } else {
            disconnectBanner.textContent = "Connection lost. Reconnecting...";
        }
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

        // Focus trap: move focus into visible modal
        if (s === "BetweenRounds" && !betweenRounds.contains(document.activeElement)) {
            betweenRounds.focus();
        }
        if (s === "GameOver" && !gameOver.contains(document.activeElement)) {
            const btn = btnPlayAgain || btnReturnLobby;
            if (btn) btn.focus();
        }
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

            // Add copy button if not present
            ensureCopyButton(lobby.roomCode);

            // Player list
            let html = "";
            for (const p of lobby.players) {
                const botTag = p.isBot ? '<span class="bot-badge">[BOT]</span>' : "";
                const removeBtn = (lobby.isLeader && p.isBot)
                    ? `<button class="bot-remove-btn" data-bot-id="${p.id}">Remove</button>`
                    : "";
                html += `<div class="player-item">
                    <span>${escapeHtml(p.name)}</span>
                    ${botTag}
                    ${p.isLeader ? '<span class="leader-badge">Leader</span>' : ""}
                    ${removeBtn}
                </div>`;
            }
            playerList.innerHTML = html;

            // Bind remove-bot buttons
            playerList.querySelectorAll(".bot-remove-btn").forEach((btn) => {
                btn.addEventListener("click", () => {
                    const botId = Number(btn.dataset.botId);
                    if (window._bpRemoveBot) window._bpRemoveBot(botId);
                });
            });

            // Add Bot button (leader only)
            let addBotBtn = $("btn-add-bot");
            if (lobby.isLeader && lobby.connected) {
                if (!addBotBtn) {
                    addBotBtn = document.createElement("button");
                    addBotBtn.id = "btn-add-bot";
                    addBotBtn.className = "btn-secondary";
                    addBotBtn.textContent = "Add Bot";
                    addBotBtn.addEventListener("click", () => {
                        if (window._bpAddBot) window._bpAddBot();
                    });
                    btnStart.parentNode.insertBefore(addBotBtn, btnStart);
                }
                addBotBtn.classList.remove("hidden");
            } else if (addBotBtn) {
                addBotBtn.classList.add("hidden");
            }

            // Start button (leader only)
            btnStart.classList.toggle("hidden", !lobby.isLeader);

            // Disable create/join after connected (use CSS :disabled)
            btnCreate.disabled = true;
            btnJoin.disabled = true;
        } else {
            roomInfo.classList.add("hidden");
            btnCreate.disabled = false;
            btnJoin.disabled = false;
        }

        // Highlight selected game button
        const sel = lobby.selectedGame || selectedGame;
        gameBtns.forEach((btn) => {
            const isSelected = btn.dataset.game === sel;
            btn.classList.toggle("selected", isSelected);
            btn.setAttribute("aria-pressed", String(isSelected));
        });
    }

    // ── Copy room code button ────────────────────────────
    let copyBtnCreated = false;
    function ensureCopyButton(roomCode) {
        if (copyBtnCreated) return;
        const codeDisplay = roomCodeValue.parentElement;
        if (!codeDisplay) return;

        const btn = document.createElement("button");
        btn.className = "room-code-copy";
        btn.textContent = "Copy";
        btn.setAttribute("aria-label", "Copy room code");
        btn.addEventListener("click", () => {
            if (navigator.clipboard && navigator.clipboard.writeText) {
                navigator.clipboard.writeText(roomCode).then(() => {
                    btn.textContent = "Copied!";
                    btn.classList.add("copied");
                    setTimeout(() => {
                        btn.textContent = "Copy";
                        btn.classList.remove("copied");
                    }, 2000);
                });
            } else {
                // Fallback: select the room code text
                const range = document.createRange();
                range.selectNodeContents(roomCodeValue);
                const sel = window.getSelection();
                sel.removeAllRanges();
                sel.addRange(range);
                btn.textContent = "Selected!";
                setTimeout(() => { btn.textContent = "Copy"; }, 2000);
            }
        });
        codeDisplay.appendChild(btn);
        copyBtnCreated = true;
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

        // Spectator badge
        updateSpectatorBadge(state);
    }

    // ── Spectator badge ──────────────────────────────────
    function updateSpectatorBadge(state) {
        let badge = $("spectator-badge");
        const isSpectator = state.lobby && state.lobby.isSpectator;
        if (isSpectator && state.appState === "InGame") {
            if (!badge) {
                badge = document.createElement("div");
                badge.id = "spectator-badge";
                badge.className = "spectator-badge";
                badge.textContent = "SPECTATOR";
                gameHud.appendChild(badge);
            }
            badge.classList.remove("hidden");
            // Hide controls hint for spectators
            hudControls.style.display = "none";
        } else {
            if (badge) badge.classList.add("hidden");
            hudControls.style.display = "";
        }
    }

    // ── Golf HUD ────────────────────────────────────────
    const golfHudEl     = $("golf-hud");
    const golfHoleName  = $("golf-hole-name");
    const golfPar       = $("golf-par");
    const golfStrokes   = $("golf-player-strokes");

    function updateGolfHud(state) {
        const hud = state.golfHud;
        if (!hud || !hud.players) {
            if (golfHudEl) golfHudEl.classList.add("hidden");
            return;
        }
        golfHudEl.classList.remove("hidden");
        golfHoleName.textContent = hud.holeName || `Hole ${(hud.holeIndex || 0) + 1}`;
        golfPar.textContent = `Par ${hud.par}`;

        let html = "";
        for (const p of hud.players) {
            const sunkClass = p.isSunk ? " sunk" : "";
            const sunkLabel = p.isSunk ? (p.sunkRank ? ` (#${p.sunkRank})` : " \u2713") : "";
            html += `<div class="hud-player-row${sunkClass}">
                <span class="name">${escapeHtml(p.name)}${sunkLabel}</span>
                <span class="value">${p.strokes}</span>
            </div>`;
        }
        golfStrokes.innerHTML = html;
    }

    // ── Platformer HUD ─────────────────────────────────
    const platformerHudEl   = $("platformer-hud");
    const platformerMode    = $("platformer-mode");
    const platformerHazard  = $("platformer-hazard");
    const platformerRankings = $("platformer-rankings");

    function updatePlatformerHud(state) {
        const hud = state.platformerHud;
        if (!hud || !hud.players) {
            if (platformerHudEl) platformerHudEl.classList.add("hidden");
            return;
        }
        platformerHudEl.classList.remove("hidden");
        platformerMode.textContent = hud.mode || "Race";
        platformerHazard.textContent = hud.mode === "Survival" ? `Hazard: ${Math.round(hud.hazardY)}` : "";

        // Checkpoint progress
        const finished = hud.finishCount || 0;
        const total = hud.players.length;
        const checkpointText = finished > 0 ? ` | Finished: ${finished}/${total}` : "";

        let html = "";
        for (const p of hud.players) {
            let cls = "";
            let status = "";
            if (p.eliminated) { cls = " eliminated"; status = "OUT"; }
            else if (p.finished) { cls = " finished"; status = p.finishRank ? `#${p.finishRank}` : "DONE"; }
            html += `<div class="hud-player-row${cls}">
                <span class="name">${escapeHtml(p.name)}</span>
                <span class="value">${status}</span>
            </div>`;
        }
        if (checkpointText) {
            html += `<div style="font-size:0.65rem;color:#667;margin-top:4px;">${checkpointText}</div>`;
        }
        platformerRankings.innerHTML = html;
    }

    // ── LaserTag HUD ───────────────────────────────────
    const lasertagHudEl  = $("lasertag-hud");
    const lasertagMode   = $("lasertag-mode");
    const lasertagTimer  = $("lasertag-timer");
    const lasertagScores = $("lasertag-scores");
    const lasertagStun   = $("lasertag-stun");

    // Kill feed tracking
    let prevLasertagTags = {};

    function updateLasertagHud(state) {
        const hud = state.lasertagHud;
        if (!hud || !hud.players) {
            if (lasertagHudEl) lasertagHudEl.classList.add("hidden");
            if (lasertagStun) lasertagStun.classList.add("hidden");
            prevLasertagTags = {};
            return;
        }
        lasertagHudEl.classList.remove("hidden");
        lasertagMode.textContent = hud.teamMode || "FFA";
        const secs = Math.ceil(hud.roundTimer || 0);
        lasertagTimer.textContent = secs > 0 ? `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}` : "";

        // Stun indicator
        if (lasertagStun) {
            lasertagStun.classList.toggle("hidden", !(hud.localStunRemaining > 0));
        }

        // Detect tag changes for kill feed
        for (const p of hud.players) {
            const prevTags = prevLasertagTags[p.id] || 0;
            if (p.tags > prevTags && prevTags > 0) {
                addKillFeedEntry(`${escapeHtml(p.name)} tagged someone!`);
            }
            prevLasertagTags[p.id] = p.tags || 0;
        }

        // Sort by tags descending
        const sorted = [...hud.players].sort((a, b) => (b.tags || 0) - (a.tags || 0));
        let html = "";

        // Show team scores if team mode
        if (hud.teamScores && Object.keys(hud.teamScores).length > 0) {
            const TEAM_COLORS = ["#7cf", "#f77", "#7f7", "#ff7"];
            html += '<div style="margin-bottom:6px;font-size:0.7rem;color:#889">';
            for (const [team, score] of Object.entries(hud.teamScores)) {
                const tc = TEAM_COLORS[parseInt(team)] || "#fff";
                html += `<span style="color:${tc}">T${parseInt(team) + 1}: ${score}</span> `;
            }
            html += "</div>";
        }

        for (const p of sorted) {
            const stunnedClass = p.stunned ? " stunned" : "";
            html += `<div class="hud-player-row${stunnedClass}">
                <span class="name">${escapeHtml(p.name)}</span>
                <span class="value">${p.tags || 0}</span>
            </div>`;
        }
        lasertagScores.innerHTML = html;
    }

    // ── Kill feed ────────────────────────────────────────
    let killFeedEl = null;
    const killFeedEntries = [];
    const MAX_KILL_FEED = 3;

    function addKillFeedEntry(text) {
        if (!killFeedEl) {
            killFeedEl = document.createElement("div");
            killFeedEl.className = "kill-feed";
            gameHud.appendChild(killFeedEl);
        }
        killFeedEntries.push({ text, time: Date.now() });
        if (killFeedEntries.length > MAX_KILL_FEED) killFeedEntries.shift();
        renderKillFeed();
    }

    function renderKillFeed() {
        if (!killFeedEl) return;
        const now = Date.now();
        // Remove entries older than 5s
        while (killFeedEntries.length > 0 && now - killFeedEntries[0].time > 5000) {
            killFeedEntries.shift();
        }
        killFeedEl.innerHTML = killFeedEntries
            .map((e) => `<div class="kill-feed-entry">${e.text}</div>`)
            .join("");
    }

    // Clean up kill feed periodically
    setInterval(renderKillFeed, 1000);

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
    let tronEliminatedEl   = null;

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
            if (tronEliminatedEl) { tronEliminatedEl.remove(); tronEliminatedEl = null; }
            tronNameEls.clear();
            return;
        }

        updateTronPlayerNames(hud.players);
        updateTronGauges(hud.players);
        updateTronEliminatedOverlay(hud.players);

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

    function updateTronEliminatedOverlay(players) {
        const local = players.find((p) => p.isLocal);
        if (local && !local.alive) {
            if (!tronEliminatedEl) {
                tronEliminatedEl = document.createElement("div");
                tronEliminatedEl.className = "tron-eliminated";
                tronEliminatedEl.textContent = "ELIMINATED";
                gameHud.appendChild(tronEliminatedEl);
            }
        } else if (tronEliminatedEl) {
            tronEliminatedEl.remove();
            tronEliminatedEl = null;
        }
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
    const SCORE_LABELS = {
        "mini-golf": "Strokes", "Golf": "Strokes",
        "laser-tag": "Tags", "LaserTag": "Tags",
        "platform-racer": "Score", "Platformer": "Score",
        "tron": "Score", "Tron": "Score",
    };

    function getScoreOpts(state, isGameOver) {
        const gameId = state.game ? state.game.gameId : selectedGame;
        const isGolf = gameId === "mini-golf" || gameId === "Golf";
        return {
            roundHistory: state.roundTracker.roundScoresHistory || null,
            scoreLabel: SCORE_LABELS[gameId] || "Score",
            isGameOver,
            isGolf,
        };
    }

    function updateScoreScreens(state) {
        if (state.appState === "BetweenRounds" && state.roundTracker) {
            renderScores(roundScores, state.roundTracker.scores, state.lobby.players, getScoreOpts(state, false));
            roundInfoEl.textContent = `Round ${state.roundTracker.currentRound} of ${state.roundTracker.totalRounds}`;
            // Between-round countdown with progress bar
            if (roundCountdown && state.betweenRoundCountdown != null) {
                const secs = Math.ceil(state.betweenRoundCountdown);
                const totalSecs = 8; // typical between-round duration
                const pct = Math.max(0, Math.min(100, (secs / totalSecs) * 100));
                roundCountdown.innerHTML = secs > 0
                    ? `Next round in ${secs}s...<div class="countdown-bar"><div class="countdown-bar-fill" style="width:${pct}%"></div></div>`
                    : "";
            } else if (roundCountdown) {
                roundCountdown.innerHTML = "";
            }
        }

        if (state.appState === "GameOver" && state.roundTracker) {
            renderScores(finalScores, state.roundTracker.scores, state.lobby.players, getScoreOpts(state, true));
            // Game-over auto-return countdown
            if (gameOverCountdown && state.gameOverCountdown != null) {
                const secs = Math.ceil(state.gameOverCountdown);
                gameOverCountdown.textContent = secs > 0 ? `Returning to lobby in ${secs}s...` : "";
            } else if (gameOverCountdown) {
                gameOverCountdown.textContent = "";
            }
        }
    }

    function renderScores(container, scores, players, opts) {
        if (!scores) {
            container.innerHTML = "<p>Waiting for scores...</p>";
            return;
        }

        const roundHistory = (opts && opts.roundHistory) || null;
        const scoreLabel = (opts && opts.scoreLabel) || "Score";
        const isGameOver = (opts && opts.isGameOver) || false;
        const isGolf = (opts && opts.isGolf) || false;

        // Convert scores object to sorted array
        const entries = Object.entries(scores)
            .map(([pid, score]) => ({
                pid: parseInt(pid),
                score,
                name: findPlayerName(parseInt(pid), players),
            }))
            .sort((a, b) => isGolf ? (a.score - b.score) : (b.score - a.score));

        const MEDALS = ["\ud83e\udd47", "\ud83e\udd48", "\ud83e\udd49"];
        const PLAYER_COLORS = ["#7cf", "#f93", "#7f7", "#f7f", "#97f", "#f90", "#0fb", "#f44"];

        let html = "";

        // Per-round header row if we have history
        if (roundHistory && roundHistory.length > 1) {
            html += `<div class="score-row score-header">
                <span class="rank"></span>
                <span class="name">Player</span>`;
            for (let r = 0; r < roundHistory.length; r++) {
                html += `<span class="round-col">R${r + 1}</span>`;
            }
            html += `<span class="score">${escapeHtml(scoreLabel)}</span></div>`;
        }

        entries.forEach((e, i) => {
            const medal = i < 3 ? MEDALS[i] : "";
            const winnerClass = (isGameOver && i === 0) ? " winner" : "";
            const colorIdx = entries.findIndex((x) => x.pid === e.pid);
            const dotColor = PLAYER_COLORS[colorIdx % PLAYER_COLORS.length];

            html += `<div class="score-row${winnerClass}">
                <span class="rank">${medal || (i + 1) + "."}</span>
                <span class="name"><span class="player-dot" style="background:${dotColor}"></span>${escapeHtml(e.name)}</span>`;

            // Per-round columns
            if (roundHistory && roundHistory.length > 1) {
                for (let r = 0; r < roundHistory.length; r++) {
                    const rScore = roundHistory[r][String(e.pid)] || 0;
                    html += `<span class="round-col">${rScore}</span>`;
                }
            }

            // Delta indicator
            let deltaHtml = "";
            if (roundHistory && roundHistory.length > 0) {
                const lastRound = roundHistory[roundHistory.length - 1];
                const delta = lastRound[String(e.pid)] || 0;
                if (delta > 0) deltaHtml = ` <span class="score-delta">+${delta}</span>`;
                else if (delta < 0) deltaHtml = ` <span class="score-delta negative">${delta}</span>`;
            }

            html += `<span class="score">${e.score}${deltaHtml}</span></div>`;
        });

        // Winner announcement
        if (isGameOver && entries.length > 0) {
            html += `<div class="winner-announce">${escapeHtml(entries[0].name)} wins!</div>`;
        }

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
    const toastTimers = new Map();
    const TOAST_AUTO_DISMISS_MS = 8000;
    const MAX_VISIBLE_TOASTS = 5;

    // Toast priority order for sorting
    const TOAST_PRIORITY_ORDER = { "Critical": 0, "Urgent": 1, "Notice": 2, "Ambient": 3 };

    function updateToasts(toasts) {
        if (!toasts) return;

        const currentIds = new Set(toasts.map((t) => t.id));

        // Remove dismissed toasts
        for (const [id, el] of activeToasts) {
            if (!currentIds.has(id)) {
                dismissToast(id);
            }
        }

        // Sort by priority
        const sorted = [...toasts].sort((a, b) =>
            (TOAST_PRIORITY_ORDER[a.priority] || 3) - (TOAST_PRIORITY_ORDER[b.priority] || 3)
        );

        // Add/update toasts (capped at MAX_VISIBLE_TOASTS)
        let visibleCount = 0;
        for (const toast of sorted) {
            if (visibleCount >= MAX_VISIBLE_TOASTS) break;
            visibleCount++;

            if (activeToasts.has(toast.id)) {
                // Update claim status
                const el = activeToasts.get(toast.id);
                const actions = el.querySelector(".toast-actions");
                if (toast.claimedBy && actions) {
                    actions.innerHTML = `<span class="toast-claimed" data-testid="toast-claimed">Claimed by ${escapeHtml(toast.claimedBy)}</span>`;
                }
            } else {
                // Create new toast
                const el = document.createElement("div");
                el.className = `toast priority-${toast.priority}`;
                el.dataset.testid = `toast-${toast.id}`;
                el.innerHTML = `
                    <div class="toast-title" data-testid="toast-title">${escapeHtml(toast.title)}</div>
                    <div class="toast-meta" data-testid="toast-meta">${escapeHtml(toast.source || "")} ${toast.actor ? "by " + escapeHtml(toast.actor) : ""}</div>
                    <div class="toast-actions" data-testid="toast-actions">
                        ${toast.claimedBy
                            ? `<span class="toast-claimed" data-testid="toast-claimed">Claimed by ${escapeHtml(toast.claimedBy)}</span>`
                            : `<button class="toast-claim-btn" data-testid="toast-claim-btn" data-event-id="${escapeHtml(toast.id)}">Claim</button>`
                        }
                    </div>`;
                // Bind claim button via addEventListener (CSP-safe, no inline onclick)
                const claimBtn = el.querySelector(".toast-claim-btn");
                if (claimBtn) {
                    const eventId = toast.id;
                    claimBtn.addEventListener("click", () => {
                        if (window._bpClaimAlert) window._bpClaimAlert(eventId);
                    });
                }
                toastContainer.appendChild(el);
                activeToasts.set(toast.id, el);

                // Auto-dismiss timer
                const timerId = setTimeout(() => dismissToast(toast.id), TOAST_AUTO_DISMISS_MS);
                toastTimers.set(toast.id, timerId);
            }
        }

        // Show overflow indicator
        let overflowEl = toastContainer.querySelector(".toast-overflow-indicator");
        const overflowCount = sorted.length - MAX_VISIBLE_TOASTS;
        if (overflowCount > 0) {
            if (!overflowEl) {
                overflowEl = document.createElement("div");
                overflowEl.className = "toast-overflow-indicator";
                toastContainer.appendChild(overflowEl);
            }
            overflowEl.textContent = `+${overflowCount} more`;
        } else if (overflowEl) {
            overflowEl.remove();
        }
    }

    function dismissToast(id) {
        const el = activeToasts.get(id);
        if (el) {
            el.classList.add("dismissing");
            setTimeout(() => {
                el.remove();
                activeToasts.delete(id);
            }, 300);
        } else {
            activeToasts.delete(id);
        }
        const timer = toastTimers.get(id);
        if (timer) { clearTimeout(timer); toastTimers.delete(id); }
    }

    function dismissAllToasts() {
        for (const [id] of activeToasts) {
            dismissToast(id);
        }
    }

    // ── Mute button ─────────────────────────────────────
    function updateMuteBtn(state) {
        if (state.muted) {
            btnMute.classList.add("muted");
            btnMute.innerHTML = "&#x1f507;";
            btnMute.setAttribute("aria-label", "Unmute audio");
        } else {
            btnMute.classList.remove("muted");
            btnMute.innerHTML = "&#x1f50a;";
            btnMute.setAttribute("aria-label", "Mute audio");
        }
    }

    // ── Helpers ─────────────────────────────────────────
    function escapeHtml(str) {
        if (!str) return "";
        return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    }

})();
