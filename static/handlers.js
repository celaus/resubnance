/* ── State ── */
const WS_URL = window.WS_URL;


let ws;
let isPlaying = false;
let currentVolume = 100;
let queueState = {};
let playerState = {};
let intervalTask;

/* ── WebSocket Connection ── */
function connectWebSocket() {
    ws = new WebSocket(WS_URL);
    ws.onopen = () => {
        console.info("WebSocket connected.");
        showToast("Connected to audio service.", "success");
        // Request initial state upon connection
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
        e(`WebSocket error: ${error}`);
        ws.close();
    };
}

function sendMessage(message) {
    if (ws && ws.readyState === WebSocket.OPEN) {
        console.info(JSON.stringify({ rq: message }));
        const msg = JSON.stringify({ rq: message });
        ws.send(msg);
    } else {
        e(`Connection error: Cannot send ${message}`);
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
            console.error(`Received odd response: ${response}`);
        }
    }

    switch (data.t) {
        case "queuestate":
            queueState = data.c || {};
            const currentTrack = queueState.meta &&
                queueState.meta[
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
        case "playlists":
            renderPlaylists(data.c);
            break;
        case "searchresults":
            renderSearchResults(data.c || []);
            break;
        case "playerstate":
            playerState = data.c;
            isPlaying = !playerState.is_paused
                && playerState.last_command !== "stop";
            console.log("isPlaying?", isPlaying);
            updateVolumeUI(playerState.volume * 100);
            updatePlayPauseUI();
            break;
        default:
            console.error("Received unknown message type:", data.t);
    }
}

/* ── Player controls ── */
async function stopPlayback() {
    const label = document.getElementById("playerStateLabel");
    label.textContent = "STOPPED";
    label.style.color = "var(--neon-pink)";
    label.style.textShadow = "var(--glow-pink)";

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

/* ── Player controls ── */
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

function updatePlayPauseUI() {
    const btn = document.getElementById("playPauseBtn");
    const label = document.getElementById("playerStateLabel");
    console.log("updating UI: isPlaying?", isPlaying);
    if (isPlaying) {
        btn.innerHTML = "&#9646;&#9646;"; // pause bars
        btn.classList.add("active");
        label.textContent = "PLAYING";
        label.style.color = "var(--neon-green)";
        label.style.textShadow = "var(--glow-green)";
    } else {
        btn.innerHTML = "&#9654;"; // play triangle
        btn.classList.remove("active");
        label.textContent = "PAUSED";
        label.style.color = "var(--text-dim)";
        label.style.textShadow = "none";
    }
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
    currentVolume = vol;
}

/* ── Search ── */
async function doSearch() {
    const query = document
        .getElementById("searchInput")
        .value.trim();
    if (!query) {
        renderSearchResults([]);
        return;
    }

    const btn = document.getElementById("searchBtn");
    const resultsList = document.getElementById("searchResults");
    btn.disabled = true;
    btn.textContent = "SEARCH";
    resultsList.innerHTML =
        '<li class="status-msg"><span class="spinner"></span> Searching...</li>';

    sendMessage({ t: "search", c: query });
}

function formatDuration(seconds) {
    if (!seconds) return "";
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${String(s).padStart(2, "0")} minutes`;
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
                <span class="result-name" title="Click to add to playlist" data-id="${escHtml(track.id)}">
                    ${escHtml(track.title)}
                </span>
                <span class="result-meta">${escHtml(track.artist)} ${track.duration ? "· " + formatDuration(track.duration) : ""}</span>
                <span class="result-added" id="added-${escHtml(track.id)}">&#10003; ADDED</span>
            `;
        li.querySelector(".result-name").addEventListener(
            "click",
            () => addToQueue(track),
        );
        list.appendChild(li);
    });
}

async function addToQueue(track) {
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

async function playNext(idx) {
    const items = queueState.meta.map((v) => v.id);

    sendMessage({
        t: "queue",
        c: {
            t: "set",
            c: { meta: items, currently_playing_position: idx },
        },
    });
    // sendMessage({ t: "player", c: { t: "set", c: { command: "next", volume: playerState.volume || 1.0 } } });
}

/* ── Playlists ── */
async function loadPlaylists() {
    // Request playlists via WebSocket
    sendMessage({ t: "playlists" });
}

function renderPlaylists(data) {
    const list = document.getElementById("playlistsList");
    // Normalise: accept array of playlists or object with playlists key
    const playlists = Array.isArray(data)
        ? data
        : data.playlists || [];

    if (!playlists.length) {
        list.innerHTML =
            '<li class="status-msg">No playlists found.</li>';
        return;
    }

    list.innerHTML = "";
    playlists.forEach((pl, idx) => {
        const name = pl.name || pl.title || `Playlist ${idx + 1}`;
        const tracks = pl.tracks || pl.songs || [];

        const li = document.createElement("li");
        li.className = "playlist-item";

        const tracksHtml = tracks.length
            ? tracks
                .map((t) => {
                    const label =
                        typeof t === "string"
                            ? t
                            : t.title ||
                            t.name ||
                            JSON.stringify(t);
                    return `<div class="playlist-track">${escHtml(label)}</div>`;
                })
                .join("")
            : '<div class="playlist-track" style="color:var(--text-dim); font-style:italic;">empty playlist</div>';

        li.innerHTML = `
                <div class="playlist-header" onclick="togglePlaylist(this.parentElement)">
                    <span class="playlist-name">${escHtml(name)}</span>
                    <span class="playlist-count">${tracks.length} track${tracks.length !== 1 ? "s" : ""}</span>
                    <span class="playlist-chevron">&#9654;</span>
                </div>
                <div class="playlist-tracks">${tracksHtml}</div>
            `;
        list.appendChild(li);
    });
}

function renderNowPlaying(track) {
    document.getElementById("playerTitle").textContent =
        track.title || "Unknown";
    document.getElementById("playerArtist").textContent =
        track.artist || "";
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
                        <span class="drag-handle">&#9776;</span>
                        <span>${escHtml(label)}</span>
                        ${isDownloading ? '<span class="spinner"></span>' : ""}
                    </div>`;
        })
        .join("") : `<p class="status-msg">Queue is empty.</p>`;

    panel.innerHTML = `
                <div style="display: flex; justify-content: space-between; align-items: center;">
                    <h2 class="panel-title" style="color: var(--neon-yellow); text-shadow: var(--glow-yellow);">
                        &#9889; Queue (${tracks.length})
                    </h2>
                    <button class="btn btn-pink" onclick="clearQueue()"
                        style="font-size: 0.55rem; padding: 0.3rem 0.6rem;">
                        &#128465; CLEAR
                    </button>
                </div>
                <div id="queueTracks" style="display: flex; flex-direction: column; gap: 0.4rem; flex: 1; overflow-y: auto; max-height: 300px;">
                    ${tracksHtml}
                </div>
            `;

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

/* ── Init ── */
connectWebSocket();
// Initial state loading is now handled by the WebSocket connection