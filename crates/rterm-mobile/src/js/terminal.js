// rterm mobile -- terminal view using canvas renderer.
// Polls get_screen_snapshot and renders styled cells via TerminalRenderer.

import { TerminalRenderer } from './canvas-grid.js';

const { invoke: termInvoke } = window.__TAURI__.core;

let activeSession = null;
let pollInterval = null;
let renderer = null;
let stickyMods = { Ctrl: false, Alt: false, locked: {} };
let lastTapTime = {};

// Modifier popover state.
let modMenuOpen = false;

// Pinch-to-zoom state.
let lastPinchDist = null;

const canvas = document.getElementById("terminal-canvas");
const termInput = document.getElementById("terminal-input");
const canvasWrapper = document.getElementById("canvas-wrapper");
const modPopover = document.getElementById("mod-popover");
const btnModMenu = document.getElementById("btn-mod-menu");
const termStatus = document.getElementById("term-status");
const termTitle = document.getElementById("term-title");

// --- Font size controls ---
document.getElementById("btn-font-sm").addEventListener("click", () => {
    if (renderer) renderer.resizeFont(-2);
});

document.getElementById("btn-font-lg").addEventListener("click", () => {
    if (renderer) renderer.resizeFont(2);
});

// --- Back button ---
document.getElementById("btn-back-term").addEventListener("click", () => {
    stopTerminal();
    // Return to host list — notify app.js.
    if (window.onStopTerminal) window.onStopTerminal();
});

// --- Mod menu toggle ---
btnModMenu.addEventListener("click", () => {
    modMenuOpen = !modMenuOpen;
    modPopover.classList.toggle("hidden", !modMenuOpen);
    btnModMenu.classList.toggle("active", modMenuOpen);
});

// --- Init renderer ---
function initRenderer() {
    renderer = new TerminalRenderer(canvas, { fontSize: 14 });
}

// --- Connection status ---
function setStatus(status) {
    termStatus.className = "status-indicator status-" + status;
    switch (status) {
        case "connected":
            termStatus.textContent = "connected";
            break;
        case "disconnected":
            termStatus.textContent = "disconnected";
            break;
        case "connecting":
        default:
            termStatus.textContent = "connecting";
            break;
    }
}

// --- Start/stop terminal polling ---
function startTerminal(sessionName) {
    activeSession = sessionName;
    initRenderer();
    setStatus("connecting");

    // Poll for snapshot every 100ms
    if (pollInterval) clearInterval(pollInterval);
    pollInterval = setInterval(pollSnapshot, 100);
    pollSnapshot();
}

function stopTerminal() {
    if (pollInterval) {
        clearInterval(pollInterval);
        pollInterval = null;
    }
    activeSession = null;
    renderer = null;
    modMenuOpen = false;
    modPopover.classList.add("hidden");
    btnModMenu.classList.remove("active");
    // Reset sticky mods.
    stickyMods = { Ctrl: false, Alt: false, locked: {} };
    updateAllModButtons();
}

async function pollSnapshot() {
    if (!activeSession) return;
    try {
        const sd = await termInvoke("get_screen_snapshot", { session: activeSession });
        if (renderer) {
            renderer.render(sd);
        }
        setStatus("connected");
    } catch (e) {
        // Session may have ended.
        console.error("get_screen_snapshot failed:", e);
        setStatus("disconnected");
    }
}

// --- Keyboard input ---
termInput.addEventListener("keydown", function (e) {
    if (!activeSession) return;

    if (e.key === "Enter") {
        e.preventDefault();
        const text = termInput.value + "\n";
        termInput.value = "";
        sendToSession(text);
    }
});

async function sendToSession(data) {
    if (!activeSession) return;

    // Apply sticky modifiers.
    let processed = data;
    if (stickyMods.Ctrl) {
        const code = processed.charCodeAt(0);
        if (code >= 97 && code <= 122) {
            processed = String.fromCharCode(code - 96);
        } else if (code >= 65 && code <= 90) {
            processed = String.fromCharCode(code - 64);
        }
        if (!stickyMods.locked.Ctrl) stickyMods.Ctrl = false;
    }
    if (stickyMods.Alt) {
        processed = "\x1b" + processed;
        if (!stickyMods.locked.Alt) stickyMods.Alt = false;
    }

    updateAllModButtons();

    try {
        await termInvoke("send_keys", { session: activeSession, data: processed });
    } catch (e) {
        console.error("send_keys failed:", e);
    }
}

// --- Key sequence map ---
const KEY_SEQUENCES = {
    Escape: "\x1b",
    Tab: "\t",
    ArrowUp: "\x1b[A",
    ArrowDown: "\x1b[B",
    ArrowRight: "\x1b[C",
    ArrowLeft: "\x1b[D",
    Home: "\x1b[H",
    End: "\x1b[F",
    PageUp: "\x1b[5~",
    PageDown: "\x1b[6~",
    Insert: "\x1b[2~",
    Delete: "\x1b[3~",
    F1: "\x1bOP",
    F2: "\x1bOQ",
    F3: "\x1bOR",
    F4: "\x1bOS",
    F5: "\x1b[15~",
    F6: "\x1b[17~",
    F7: "\x1b[18~",
    F8: "\x1b[19~",
    F9: "\x1b[20~",
    F10: "\x1b[21~",
    F11: "\x1b[23~",
    F12: "\x1b[24~",
};

// --- Accessory bar ---
document.querySelectorAll(".accessory-bar button, .mod-popover button").forEach(function (btn) {
    btn.addEventListener("click", handleAccBtn);
});

function handleAccBtn(e) {
    if (!activeSession) return;
    e.preventDefault();

    const mod = btn.getAttribute ? btn.getAttribute("data-mod") : null;
    const key = btn.getAttribute ? btn.getAttribute("data-key") : null;
    const lockable = btn.getAttribute ? btn.getAttribute("data-lockable") === "true" : false;
    const btnEl = e.currentTarget;

    if (mod) {
        // Double-tap detection for locking.
        const now = Date.now();
        const lastTap = lastTapTime[mod] || 0;
        if (now - lastTap < 300 && lockable) {
            // Double-tap: toggle lock.
            stickyMods.locked[mod] = !stickyMods.locked[mod];
            stickyMods[mod] = !!stickyMods.locked[mod];
        } else {
            // Single tap: arm sticky.
            stickyMods[mod] = !stickyMods[mod];
            stickyMods.locked[mod] = false;
        }
        lastTapTime[mod] = now;
        updateAllModButtons();
        return;
    }

    if (!key) return;

    const seq = KEY_SEQUENCES[key] || key;
    sendToSession(seq);

    // Auto-disarm non-locked sticky mods.
    if (stickyMods.Ctrl && !stickyMods.locked.Ctrl) {
        stickyMods.Ctrl = false;
        updateAllModButtons();
    }
    if (stickyMods.Alt && !stickyMods.locked.Alt) {
        stickyMods.Alt = false;
        updateAllModButtons();
    }
}

function updateAllModButtons() {
    document.querySelectorAll(".accessory-bar button[data-mod]").forEach(function (btn) {
        const mod = btn.getAttribute("data-mod");
        const locked = stickyMods.locked && stickyMods.locked[mod];
        const active = stickyMods[mod];
        btn.classList.toggle("active", !!active && !locked);
        btn.classList.toggle("locked", !!locked);
    });
}

// --- Pinch-to-zoom on canvas ---
let initialPinchScale = 1;

canvasWrapper.addEventListener("touchstart", function (e) {
    if (e.touches.length === 2) {
        lastPinchDist = Math.hypot(
            e.touches[0].clientX - e.touches[1].clientX,
            e.touches[0].clientY - e.touches[1].clientY
        );
        initialPinchScale = renderer ? 1 : 1;
    }
}, { passive: true });

canvasWrapper.addEventListener("touchmove", function (e) {
    if (e.touches.length === 2 && lastPinchDist !== null) {
        const dist = Math.hypot(
            e.touches[0].clientX - e.touches[1].clientX,
            e.touches[0].clientY - e.touches[1].clientY
        );
        const scale = dist / lastPinchDist;
        if (renderer && scale !== 1) {
            renderer.resizeFont(scale > 1 ? 1 : -1);
        }
        lastPinchDist = dist;
    }
}, { passive: true });

canvasWrapper.addEventListener("touchend", function (e) {
    if (e.touches.length < 2) {
        lastPinchDist = null;
    }
}, { passive: true });

// --- Hardware keyboard: hide accessory bar when typing ---
const isMobile = /iPhone|iPad|iPod|Android/i.test(navigator.userAgent);
if (isMobile) {
    termInput.addEventListener("focus", () => {
        // On mobile, the keyboard appearing means we may want to hide
        // the accessory bar to save space. But keep it for now as it's
        // useful for touch typing of special keys.
    });
}

// --- Expose stopTerminal globally for app.js ---
window.stopTerminal = stopTerminal;
window.startTerminal = startTerminal;
window.setStatus = setStatus;
