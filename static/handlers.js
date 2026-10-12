/* ── State ── */
const WS_URL = window.WS_URL;

let ws;
let isPlaying = false;
let queueState = { meta: [], currently_playing_position: null };
let playerState = {};
let intervalTask;

/* ── WebSocket Connection ── */
function connectWebSocket() {
    ws = new WebSocket(WS_URL);
    ws.onopen = () => {
        console.info("WebSocket connected.");
        showToast("Connected to audio service.", "success");
    };

    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        handleWebSocketMessage(data);
    };

    ws.onclose = (event) => {
        console.warn(
            "WebSocket disconnected. Attempting to reconnect in 500 ms...",
            event,
        );
        showToast("Connection lost. Reconnecting...", "error");
        clearInterval(intervalTask);
        setTimeout(connectWebSocket, 500);
    };

    ws.onerror = (error) => {
        e(`WebSocket error: ${JSON.stringify(error)}`);
        ws.close();
    };
}

function sendMessage(message) {
    if (ws && ws.readyState === WebSocket.OPEN) {
        const msg = JSON.stringify({ rq: message });
        console.info(msg);
        ws.send(msg);
    } else {
        e(`Connection error: Cannot send ${JSON.stringify(message)}`);
    }
}

function e(msg) {
    console.error(msg);
    showToast(msg, "error");
}

/* ── Toast ── */
let toastTimer = null;
function showToast(msg, type = "success") {
    const el = document.getElementById("toast");
    el.textContent = msg;
    el.className = "show " + type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
        el.className = "";
    }, 2500);
}

/* ── WebSocket Message Handler ── */
function handleWebSocketMessage(response) {
    console.debug(response);
    const data = response.rp;
    const err = response.error;
    if (!data && err) {
        e(err);
        return;
    } else {
        if (!data && !err) {
            console.error(`Received odd response: ${JSON.stringify(response)}`);
        }
    }

    switch (data.t) {
        case "queuestate":
            queueState = data.c || {};
            if (!Array.isArray(queueState.meta)) queueState.meta = [];
            const currentTrack = queueState.meta[
                queueState.currently_playing_position
                ];
            if (currentTrack != undefined) {
                renderNowPlaying(currentTrack);
            } else {
                isPlaying = false;
            }
            updatePlayPauseUI();
            renderQueue(queueState);
            break;
        case "searchresults":
            renderSearchResults(data.c || []);
            break;
        case "playerstate":
            playerState = data.c;
            isPlaying = !playerState.is_paused
                && playerState.last_command !== "stop";
            updateVolumeUI(playerState.volume * 100);
            updatePlayPauseUI();
            break;
        default:
            console.error("Received unknown message type:", data.t);
    }
}

/* ── Player controls ── */
async function stopPlayback() {
    sendMessage({
        t: "player",
        c: {
            t: "set",
            c: {
                command: "stop",
                volume: playerState.volume || 1.0,
            },
        },
    });
}

async function togglePlayPause() {
    const command = isPlaying ? "pause" : "play";
    sendMessage({
        t: "player",
        c: {
            t: "set",
            c: {
                command: command,
                volume: playerState.volume || 1.0,
            },
        },
    });
}

const PLAY_ICON =
    '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z" /></svg>';
const PAUSE_ICON =
    '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 5h4v14H6zM14 5h4v14h-4z" /></svg>';

function updatePlayPauseUI() {
    const btn = document.getElementById("playPauseBtn");
    if (isPlaying) {
        btn.innerHTML = PAUSE_ICON;
        btn.setAttribute("aria-label", "Pause");
        btn.classList.add("active");
    } else {
        btn.innerHTML = PLAY_ICON;
        btn.setAttribute("aria-label", "Play");
        btn.classList.remove("active");
    }
    document.body.classList.toggle("is-playing", isPlaying);
    updateProgressUI();
}

async function skipNext() {
    sendMessage({
        t: "player",
        c: {
            t: "set",
            c: {
                command: "next",
                volume: playerState.volume || 1.0,
            },
        },
    });
    // State update handled by 'playback_status' message
}

function onVolumeChange(value) {
    const vol = parseFloat(value);
    document.getElementById("volumeValue").textContent =
        Math.round(vol) + "%";
}

function onVolumeCommit(value) {
    const vol = parseFloat(value) / 100.0;
    playerState.volume = vol;

    sendMessage({
        t: "player",
        c: {
            t: "set",
            c: {
                command: "volume",
                volume: playerState.volume,
            },
        },
    });
}

function updateVolumeUI(vol) {
    const slider = document.getElementById("volumeSlider");
    const label = document.getElementById("volumeValue");
    slider.value = vol;
    label.textContent = Math.round(vol) + "%";
}

/* ── Progress (dummy — the server does not report playback position yet) ── */
let dummyPosition = 0;
let dummyTotal = 0;
let lastTrackId = null;

function formatTime(seconds) {
    if (typeof seconds !== "number" || !isFinite(seconds) || seconds < 0) {
        return "–:––";
    }
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return `${m}:${String(s).padStart(2, "0")}`;
}

function updateProgressUI() {
    const fill = document.getElementById("progressFill");
    if (!fill) return;
    fill.style.width =
        dummyTotal > 0
            ? `${Math.min(100, (dummyPosition / dummyTotal) * 100)}%`
            : "0%";
    const elapsed = document.getElementById("elapsedTime");
    const total = document.getElementById("totalTime");
    if (elapsed) elapsed.textContent = formatTime(dummyPosition);
    if (total) total.textContent = formatTime(dummyTotal);
}

intervalTask = setInterval(() => {
    if (isPlaying && dummyTotal > 0 && dummyPosition < dummyTotal) {
        dummyPosition = Math.min(dummyPosition + 1, dummyTotal);
        updateProgressUI();
    }
}, 1000);

/* ── Search ── */
async function doSearch() {
    const query = document
        .getElementById("searchInput")
        .value.trim();
    if (!query) {
        renderSearchResults([]);
        return;
    }

    const resultsList = document.getElementById("searchResults");
    resultsList.innerHTML =
        '<li class="status-msg"><span class="spinner"></span> Searching...</li>';

    sendMessage({ t: "search", c: query });
}

function renderSearchResults(tracks) {
    const list = document.getElementById("searchResults");
    if (!tracks.length) {
        list.innerHTML =
            '<li class="status-msg">No tracks found.</li>';
        return;
    }
    list.innerHTML = "";
    tracks.forEach((track) => {
        const li = document.createElement("li");
        li.className = "result-item";
        li.innerHTML = `
                <span class="result-name" title="Click to add to queue" data-id="${escHtml(track.id)}">
                    ${escHtml(track.title)}
                </span>
                <span class="result-meta">${escHtml(track.artist)} ${track.duration ? "· " + formatTime(track.duration) : ""}</span>
                <span class="result-added" id="added-${escHtml(track.id)}">added</span>
            `;
        li.querySelector(".result-name").addEventListener(
            "click",
            () => addToQueue(track),
        );
        list.appendChild(li);
    });
}

async function addToQueue(track) {
    if (!Array.isArray(queueState.meta)) queueState.meta = [];
    queueState.meta.push(track);
    const items = queueState.meta.map((v) => v.id);

    sendMessage({
        t: "queue",
        c: {
            t: "set",
            c: {
                meta: items,
                currently_playing_position:
                    queueState.currently_playing_position,
            },
        },
    });
    // Success/Failure handled by 'search_results' or dedicated 'add_status' message
    showToast(`Adding "${track.title}"...`, "success");
}

let currentTitle = "not playing";

/* ── Marquee for long titles ── */
function setupMarquee(el, text) {
    if (!el) return;
    el.classList.remove("marquee");
    el.textContent = text;
    if (el.scrollWidth <= el.clientWidth) return;

    el.innerHTML =
        `<span class="marquee-inner"><span>${escHtml(text)}</span>` +
        `<span aria-hidden="true">${escHtml(text)}</span></span>`;
    el.classList.add("marquee");

    const inner = el.querySelector(".marquee-inner");
    if (inner) {
        const gap = 40;
        const duration = Math.max(8, (el.scrollWidth / 2 + gap) / 35);
        inner.style.animationDuration = `${duration}s`;
    }
}

function renderNowPlaying(track) {
    currentTitle = track.title || "Unknown";
    setupMarquee(document.getElementById("playerTitle"), currentTitle);
    document.getElementById("playerArtist").textContent =
        track.artist || "";
    if (track.id !== lastTrackId) {
        lastTrackId = track.id;
        dummyPosition = 0;
    }
    const duration = Number(track.duration);
    dummyTotal = isFinite(duration) && duration > 0 ? duration : 210;
    updateProgressUI();
}

function renderQueue(queue) {
    const data = queue.meta;
    const panel = document.getElementById("queuePanel");
    const tracks = Array.isArray(data) ? data : [];

    const tracksHtml = tracks.length ? tracks
        .map((t, idx) => {
            const isPlaying = idx === queueState.currently_playing_position;
            const isDownloading = queue.downloading && queue.downloading.includes(t.id);
            const label_ = (t.title || t.name || JSON.stringify(t));
            const label = label_.length > 100 ? label_.substring(0, 97) + "..." : label_;

            return `<div class="queue-track ${isPlaying ? "playing" : ""}" draggable="true" data-idx="${idx}" data-id="${escHtml(t.id)}">
                        <span class="drag-handle">&#8942;</span>
                        <span class="eq"><i></i><i></i><i></i></span>
                        <span class="queue-label">${escHtml(label)}</span>
                        ${isDownloading ? '<span class="spinner spinner-sm"></span>' : ""}
                    </div>`;
        })
        .join("") : `<p class="status-msg">Queue is empty.</p>`;

    panel.innerHTML = `
                <div class="queue-header">
                    <h2 class="panel-title">Queue${tracks.length ? ` (${tracks.length})` : ""}</h2>
                    <button class="btn btn-sm" id="queueClearBtn">Clear</button>
                </div>
                <div id="queueTracks" class="queue-tracks">
                    ${tracksHtml}
                </div>
            `;

    const clearBtn = document.getElementById("queueClearBtn");
    if (clearBtn) clearBtn.addEventListener("click", clearQueue);
    setupQueueDragDrop();
}

async function clearQueue() {
    if (confirm("Are you sure you want to clear the queue?")) {
        queueState.meta = [];
        queueState.currently_playing_position = null;
        sendMessage({ t: "queue", c: { t: "set", c: queueState } });
        stopPlayback();
        showToast("Clearing queue...", "success");
    }
}

/* ── Drag-and-drop queue reordering ── */
function setupQueueDragDrop() {
    const container = document.getElementById("queueTracks");
    if (!container) return;

    let dragSrcIdx = null;

    const handleDragStart = (e) => {
        const track = e.target.closest(".queue-track");
        if (!track) return;
        dragSrcIdx = parseInt(track.dataset.idx);
        track.classList.add("dragging");
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", String(dragSrcIdx));
    };

    const handleDragEnd = (e) => {
        const track = e.target.closest(".queue-track");
        if (track) track.classList.remove("dragging");
        container.querySelectorAll(".queue-track").forEach(el => el.classList.remove("drag-over"));
        dragSrcIdx = null;
    };

    const handleDragOver = (e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        const track = e.target.closest(".queue-track");
        if (!track) return;
        container.querySelectorAll(".queue-track").forEach(el => el.classList.remove("drag-over"));
        track.classList.add("drag-over");
    };

    const handleDrop = (e) => {
        e.preventDefault();
        const track = e.target.closest(".queue-track");
        if (!track) return;
        const toIdx = parseInt(track.dataset.idx);
        if (dragSrcIdx === null || dragSrcIdx === toIdx) return;

        const items = queueState.meta;
        const [moved] = items.splice(dragSrcIdx, 1);
        items.splice(toIdx, 0, moved);

        let cpp = queueState.currently_playing_position;
        if (dragSrcIdx === cpp) {
            cpp = toIdx;
        } else if (dragSrcIdx < cpp && toIdx >= cpp) {
            cpp--;
        } else if (dragSrcIdx > cpp && toIdx <= cpp) {
            cpp++;
        }

        sendMessage({
            t: "queue",
            c: {
                t: "set",
                c: {
                    meta: items.map(v => v.id),
                    currently_playing_position: cpp,
                },
            },
        });

        renderQueue(queueState);
    };

    container.addEventListener("dragstart", handleDragStart);
    container.addEventListener("dragend", handleDragEnd);
    container.addEventListener("dragover", handleDragOver);
    container.addEventListener("drop", handleDrop);
}

/* ── Helpers ── */
function escHtml(str) {
    if (str == null) return "";
    return String(str)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}

/* ── Search on Enter key ── */
document
    .getElementById("searchInput")
    .addEventListener("keydown", (e) => {
        if (e.key === "Enter") doSearch();
    });

/* Keep the search box focused while clicking a result so the overlay stays visible */
document
    .getElementById("searchResults")
    .addEventListener("mousedown", (e) => e.preventDefault());

/* ── Control wiring ── */
document
    .getElementById("playPauseBtn")
    .addEventListener("click", togglePlayPause);
document
    .getElementById("skipBtn")
    .addEventListener("click", skipNext);
document
    .getElementById("stopBtn")
    .addEventListener("click", stopPlayback);
document
    .getElementById("queueClearBtn")
    .addEventListener("click", clearQueue);

const volumeSlider = document.getElementById("volumeSlider");
volumeSlider.addEventListener("input", (e) => onVolumeChange(e.target.value));
volumeSlider.addEventListener("change", (e) => onVolumeCommit(e.target.value));

/* ── Global keyboard & media session handlers ── */
document.addEventListener("keydown", (e) => {
    // Don't intercept when typing in search
    if (e.target === document.getElementById("searchInput")) return;
    switch (e.key) {
        case " ":
            e.preventDefault();
            togglePlayPause();
            break;
        case "MediaTrackNext":
        case "MediaFastForward":
            e.preventDefault();
            skipNext();
            break;
        case "MediaTrackPrevious":
        case "MediaRewind":
            e.preventDefault();
            break;
        case "MediaPlay":
            e.preventDefault();
            if (!isPlaying) togglePlayPause();
            break;
        case "MediaPause":
            e.preventDefault();
            if (isPlaying) togglePlayPause();
            break;
        case "MediaStop":
            e.preventDefault();
            stopPlayback();
            break;
    }
});

if ("mediaSession" in navigator) {
    navigator.mediaSession.setActionHandler("play", () => {
        if (!isPlaying) togglePlayPause();
    });
    navigator.mediaSession.setActionHandler("pause", () => {
        if (isPlaying) togglePlayPause();
    });
    navigator.mediaSession.setActionHandler("stop", () => {
        stopPlayback();
    });
    navigator.mediaSession.setActionHandler("nexttrack", () => {
        skipNext();
    });
    navigator.mediaSession.setActionHandler("previoustrack", () => {
        // no previous track action defined
    });
}

/* ── Re-evaluate the marquee when the layout width changes ── */
let marqueeResizeTimer = null;
window.addEventListener("resize", () => {
    clearTimeout(marqueeResizeTimer);
    marqueeResizeTimer = setTimeout(() => {
        setupMarquee(document.getElementById("playerTitle"), currentTitle);
    }, 150);
});

/* ── Init ── */
connectWebSocket();