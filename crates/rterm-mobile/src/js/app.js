// rterm mobile -- host list and navigation
// Uses window.__TAURI__ for invoke() calls to the Rust backend.

const { invoke } = window.__TAURI__.core;

// --- State ---
let hosts = [];
let currentSessionName = null;

// --- DOM refs ---
const pageHosts = document.getElementById("page-hosts");
const pageEdit = document.getElementById("page-edit");
const pageTerminal = document.getElementById("page-terminal");

const hostList = document.getElementById("host-list");
const hostEmpty = document.getElementById("host-empty");

const editTitle = document.getElementById("edit-title");
const hostForm = document.getElementById("host-form");
const fieldId = document.getElementById("host-id");
const fieldName = document.getElementById("host-name");
const fieldHostname = document.getElementById("host-hostname");
const fieldPort = document.getElementById("host-port");
const fieldUsername = document.getElementById("host-username");
const fieldAuthType = document.getElementById("host-auth-type");
const fieldPassword = document.getElementById("host-password");
const passwordField = document.getElementById("password-field");
const btnDeleteHost = document.getElementById("btn-delete-host");

// --- Navigation ---
function showPage(page) {
    pageHosts.classList.add("hidden");
    pageEdit.classList.add("hidden");
    pageTerminal.classList.add("hidden");
    page.classList.remove("hidden");
}

// --- Host list ---
async function refreshHosts() {
    try {
        hosts = await invoke("load_hosts");
    } catch (e) {
        console.error("load_hosts failed:", e);
        hosts = [];
    }
    renderHostList();
}

function renderHostList() {
    hostList.innerHTML = "";
    if (hosts.length === 0) {
        hostEmpty.style.display = "";
        return;
    }
    hostEmpty.style.display = "none";

    for (const host of hosts) {
        const item = document.createElement("div");
        item.className = "host-item";

        const info = document.createElement("div");
        info.className = "host-item-info";
        info.innerHTML =
            '<div class="host-item-name">' + escapeHtml(host.name) + "</div>" +
            '<div class="host-item-detail">' +
            escapeHtml(host.username) + "@" + escapeHtml(host.hostname) + ":" + host.port +
            "</div>";

        info.addEventListener("click", function () {
            connectToHost(host);
        });

        const editBtn = document.createElement("button");
        editBtn.className = "host-item-edit";
        editBtn.textContent = "...";
        editBtn.addEventListener("click", function (e) {
            e.stopPropagation();
            openEditHost(host);
        });

        item.appendChild(info);
        item.appendChild(editBtn);
        hostList.appendChild(item);
    }
}

function escapeHtml(str) {
    const div = document.createElement("div");
    div.textContent = str;
    return div.innerHTML;
}

// --- Edit host ---
function openAddHost() {
    editTitle.textContent = "Add Host";
    fieldId.value = "";
    fieldName.value = "";
    fieldHostname.value = "";
    fieldPort.value = "22";
    fieldUsername.value = "";
    fieldAuthType.value = "password";
    fieldPassword.value = "";
    passwordField.style.display = "";
    btnDeleteHost.classList.add("hidden");
    showPage(pageEdit);
}

function openEditHost(host) {
    editTitle.textContent = "Edit Host";
    fieldId.value = host.id;
    fieldName.value = host.name;
    fieldHostname.value = host.hostname;
    fieldPort.value = host.port;
    fieldUsername.value = host.username;
    fieldAuthType.value = host.auth_type;
    fieldPassword.value = host.password || "";
    passwordField.style.display = host.auth_type === "password" ? "" : "none";
    btnDeleteHost.classList.remove("hidden");
    showPage(pageEdit);
}

async function saveHost() {
    const id = fieldId.value || crypto.randomUUID();
    const host = {
        id: id,
        name: fieldName.value.trim(),
        hostname: fieldHostname.value.trim(),
        port: parseInt(fieldPort.value, 10) || 22,
        username: fieldUsername.value.trim(),
        auth_type: fieldAuthType.value,
        password: fieldAuthType.value === "password" ? fieldPassword.value : null,
    };

    if (!host.name || !host.hostname || !host.username) {
        return; // basic validation
    }

    try {
        await invoke("save_host", { host: host });
    } catch (e) {
        console.error("save_host failed:", e);
        return;
    }

    await refreshHosts();
    showPage(pageHosts);
}

async function deleteCurrentHost() {
    const id = fieldId.value;
    if (!id) return;
    try {
        await invoke("delete_host", { id: id });
    } catch (e) {
        console.error("delete_host failed:", e);
    }
    await refreshHosts();
    showPage(pageHosts);
}

// --- Connect to host ---
async function connectToHost(host) {
    const sessionName = host.name + "-" + Date.now();
    const sshUri =
        "ssh://" + host.username +
        (host.password ? ":" + host.password : "") +
        "@" + host.hostname + ":" + host.port;

    try {
        await invoke("create_session", {
            name: sessionName,
            sshUri: sshUri,
            cols: 80,
            rows: 24,
        });
    } catch (e) {
        console.error("create_session failed:", e);
        return;
    }

    currentSessionName = sessionName;
    showPage(pageTerminal);
    startTerminal(sessionName);
}

// --- Auth type toggle ---
fieldAuthType.addEventListener("change", function () {
    passwordField.style.display = fieldAuthType.value === "password" ? "" : "none";
});

// --- Event listeners ---
document.getElementById("btn-add-host").addEventListener("click", openAddHost);
document.getElementById("btn-back-edit").addEventListener("click", function () {
    showPage(pageHosts);
});
document.getElementById("btn-save-host").addEventListener("click", saveHost);
document.getElementById("btn-delete-host").addEventListener("click", deleteCurrentHost);

// --- Init ---
refreshHosts();

// --- Terminal back button handler ---
window.onStopTerminal = function () {
    stopTerminal();
    showPage(pageHosts);
};
