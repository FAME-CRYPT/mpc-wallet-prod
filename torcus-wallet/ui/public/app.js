// MPC Wallet UI - JavaScript Application

// Global state
let currentTab = "dashboard";
let wallets = [];
let coordinatorUrl =
  localStorage.getItem("coordinatorUrl") || "http://localhost:3000";

// Initialize app
document.addEventListener("DOMContentLoaded", () => {
  document.getElementById("coordinator-url").value = coordinatorUrl;
  loadDashboard();
  showTab("dashboard");
});

// Tab navigation
function showTab(tabName) {
  currentTab = tabName;

  // Hide all tabs
  document.querySelectorAll(".tab-content").forEach((tab) => {
    tab.classList.add("hidden");
  });

  // Show selected tab
  const selectedTab = document.getElementById(`${tabName}-tab`);
  if (selectedTab) {
    selectedTab.classList.remove("hidden");
  }

  // Update nav buttons
  document.querySelectorAll(".nav-btn").forEach((btn) => {
    btn.classList.remove("bg-gray-100", "text-blue-600");
    btn.classList.add("text-gray-700");
  });
  event?.target?.classList?.add("bg-gray-100", "text-blue-600");

  // Load tab-specific data
  if (tabName === "dashboard") {
    loadDashboard();
  } else if (tabName === "wallets") {
    loadWallets();
  } else if (tabName === "send") {
    loadSendWallets();
  } else if (tabName === "settings") {
    loadSystemInfo();
    loadMiningWallets();
  }
}

// API Helper
async function apiCall(endpoint, options = {}) {
  try {
    const response = await fetch(`${coordinatorUrl}${endpoint}`, {
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(errorText || `HTTP ${response.status}`);
    }

    return await response.json();
  } catch (error) {
    console.error("API Error:", error);
    showToast(`Error: ${error.message}`, "error");
    throw error;
  }
}

// Dashboard
async function loadDashboard() {
  try {
    const data = await apiCall("/wallets");
    wallets = data.wallets || [];

    // Update stats
    document.getElementById("wallet-count").textContent = wallets.length;

    // Calculate total balance
    let totalSats = 0;
    for (const wallet of wallets) {
      try {
        const balance = await apiCall(`/wallet/${wallet.wallet_id}/balance`);
        totalSats += balance.total_sats || 0;
      } catch (err) {
        console.error(`Failed to get balance for ${wallet.wallet_id}:`, err);
      }
    }

    const btc = (totalSats / 100000000).toFixed(8);
    document.getElementById("total-balance").innerHTML =
      `${totalSats.toLocaleString()} sats<br><span class="text-sm text-gray-500">${btc} BTC</span>`;

    // Display wallets
    displayDashboardWallets();
  } catch (error) {
    document.getElementById("dashboard-wallets").innerHTML =
      '<div class="text-center text-red-500 py-8">Failed to load wallets. Check if coordinator is running.</div>';
  }
}

function displayDashboardWallets() {
  const container = document.getElementById("dashboard-wallets");

  if (wallets.length === 0) {
    container.innerHTML = `
            <div class="text-center py-8">
                <svg class="mx-auto w-16 h-16 text-gray-400 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z"/>
                </svg>
                <p class="text-gray-500 mb-4">No wallets yet</p>
                <button onclick="showTab('wallets')" class="bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-lg">
                    Create Your First Wallet
                </button>
            </div>
        `;
    return;
  }

  container.innerHTML = `
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
            ${wallets
              .map(
                (wallet) => `
                <div class="wallet-card bg-gray-50 rounded-lg p-4 border border-gray-200">
                    <div class="flex justify-between items-start mb-2">
                        <h4 class="font-semibold text-gray-900">${wallet.name}</h4>
                        <span class="text-xs bg-blue-100 text-blue-800 px-2 py-1 rounded">${wallet.wallet_type}</span>
                    </div>
                    <p class="text-sm font-mono text-gray-600 mb-2 break-all">${wallet.address}</p>
                    <p class="text-xs text-gray-500">ID: ${wallet.wallet_id}</p>
                </div>
            `,
              )
              .join("")}
        </div>
    `;
}

// Wallets Management
async function loadWallets() {
  try {
    const data = await apiCall("/wallets");
    wallets = data.wallets || [];
    displayWallets();
  } catch (error) {
    document.getElementById("wallets-list").innerHTML =
      '<div class="text-center text-red-500 py-8">Failed to load wallets</div>';
  }
}

function displayWallets() {
  const container = document.getElementById("wallets-list");

  if (wallets.length === 0) {
    container.innerHTML = `
            <div class="text-center py-12">
                <svg class="mx-auto w-16 h-16 text-gray-400 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z"/>
                </svg>
                <p class="text-gray-500 mb-4">No wallets found</p>
                <button onclick="showCreateWallet()" class="bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-lg">
                    Create Your First Wallet
                </button>
            </div>
        `;
    return;
  }

  container.innerHTML = wallets
    .map(
      (wallet) => `
        <div class="wallet-card bg-white rounded-lg shadow p-6">
            <div class="flex justify-between items-start">
                <div class="flex-1">
                    <div class="flex items-center mb-2">
                        <h3 class="text-lg font-semibold text-gray-900">${wallet.name}</h3>
                        <span class="ml-3 text-xs bg-blue-100 text-blue-800 px-2 py-1 rounded">${wallet.wallet_type}</span>
                    </div>
                    <p class="text-sm text-gray-600 mb-1">Address:</p>
                    <p class="text-sm font-mono text-gray-900 bg-gray-50 p-2 rounded break-all">${wallet.address}</p>
                    <p class="text-xs text-gray-500 mt-2">ID: ${wallet.wallet_id}</p>
                    ${wallet.created_at ? `<p class="text-xs text-gray-500">Created: ${new Date(wallet.created_at).toLocaleString()}</p>` : ""}
                </div>
                <div class="ml-4 flex flex-col space-y-2">
                    <button onclick="viewWalletDetails('${wallet.wallet_id}')" class="text-blue-600 hover:text-blue-800 text-sm font-medium">
                        View Details
                    </button>
                    <button onclick="deleteWallet('${wallet.wallet_id}')" class="text-red-600 hover:text-red-800 text-sm font-medium">
                        Delete
                    </button>
                </div>
            </div>
        </div>
    `,
    )
    .join("");
}

function showCreateWallet() {
  document.getElementById("create-wallet-form").classList.remove("hidden");
  document.getElementById("wallet-name").focus();
}

function hideCreateWallet() {
  document.getElementById("create-wallet-form").classList.add("hidden");
  document.getElementById("wallet-name").value = "";
  document.getElementById("wallet-threshold").value = "3";
}

async function createWallet(event) {
  event.preventDefault();

  const name = document.getElementById("wallet-name").value;
  const threshold = parseInt(document.getElementById("wallet-threshold").value);
  const walletType = document.querySelector(
    'input[name="wallet-type"]:checked',
  ).value;

  // Check if CGGMP24 is initialized for CGGMP24 wallets
  if (walletType === "cggmp24") {
    try {
      const status = await apiCall("/nodes/status");
      if (!status.all_cggmp24_ready) {
        showToast(
          "Please initialize CGGMP24 nodes first (Settings tab)",
          "error",
        );
        return;
      }
    } catch (err) {
      showToast(
        "Cannot check node status. Make sure nodes are running.",
        "error",
      );
      return;
    }
  }

  // Show loading state
  document.getElementById("create-btn-text").classList.add("hidden");
  document.getElementById("create-btn-loading").classList.remove("hidden");

  try {
    // Step 1: Create wallet record with correct wallet type
    // Backend expects lowercase: 'bitcoin', 'taproot', 'ethereum'
    const walletTypeBackend = walletType === "taproot" ? "taproot" : "bitcoin";

    const walletRecord = await apiCall("/wallet", {
      method: "POST",
      body: JSON.stringify({
        name: name,
        wallet_type: walletTypeBackend,
      }),
    });

    const walletId = walletRecord.wallet_id;

    // Step 2: Start key generation
    if (walletType === "cggmp24") {
      await apiCall("/cggmp24/keygen/start", {
        method: "POST",
        body: JSON.stringify({
          wallet_id: walletId,
          threshold: threshold,
        }),
      });

      showToast(
        "CGGMP24 wallet creation started! This may take a minute...",
        "success",
      );

      // Poll for completion
      await pollWalletCreation(walletId, "cggmp24");
    } else {
      // Taproot/FROST
      await apiCall("/frost/keygen/start", {
        method: "POST",
        body: JSON.stringify({
          wallet_id: walletId,
          threshold: threshold,
        }),
      });

      showToast(
        "Taproot wallet creation started! This may take a minute...",
        "success",
      );

      // Poll for completion
      await pollWalletCreation(walletId, "taproot");
    }

    showToast("Wallet created successfully!", "success");
    hideCreateWallet();
    loadWallets();
    loadDashboard();
  } catch (error) {
    showToast(`Failed to create wallet: ${error.message}`, "error");
  } finally {
    document.getElementById("create-btn-text").classList.remove("hidden");
    document.getElementById("create-btn-loading").classList.add("hidden");
  }
}

async function pollWalletCreation(walletId, type) {
  const endpoint =
    type === "cggmp24" ? "/cggmp24/keyshares" : "/frost/keyshares";

  for (let i = 0; i < 60; i++) {
    await new Promise((resolve) => setTimeout(resolve, 5000)); // Wait 5 seconds

    try {
      const shares = await apiCall(endpoint);
      const keyshares = shares.keyshares || [];

      const walletShare = keyshares.find((ks) => ks.wallet_id === walletId);

      if (walletShare) {
        const nodesCount = walletShare.nodes_with_shares?.length || 0;
        const numParties = walletShare.num_parties || 4;

        if (nodesCount >= numParties) {
          // Keygen complete, update wallet with public key
          if (walletShare.public_key) {
            const updateEndpoint =
              type === "cggmp24"
                ? `/wallet/${walletId}/pubkey`
                : `/wallet/${walletId}/taproot-pubkey`;

            await apiCall(updateEndpoint, {
              method: "PUT",
              body: JSON.stringify({ public_key: walletShare.public_key }),
            });
          }
          return; // Success
        }
      }
    } catch (err) {
      console.error("Polling error:", err);
    }
  }

  throw new Error("Wallet creation timeout - check manually");
}

async function viewWalletDetails(walletId) {
  try {
    const wallet = await apiCall(`/wallet/${walletId}`);
    const balance = await apiCall(`/wallet/${walletId}/balance`);

    alert(
      `Wallet Details:\n\nName: ${wallet.name}\nType: ${wallet.wallet_type}\nAddress: ${wallet.address}\nBalance: ${balance.total_sats} sats (${balance.total_btc} BTC)\nPublic Key: ${wallet.public_key}`,
    );
  } catch (error) {
    showToast("Failed to load wallet details", "error");
  }
}

async function deleteWallet(walletId) {
  if (
    !confirm(
      "Are you sure you want to delete this wallet? This cannot be undone.",
    )
  ) {
    return;
  }

  try {
    await apiCall(`/wallet/${walletId}`, { method: "DELETE" });
    showToast("Wallet deleted successfully", "success");
    loadWallets();
    loadDashboard();
  } catch (error) {
    showToast("Failed to delete wallet", "error");
  }
}

// Send Bitcoin
async function loadSendWallets() {
  try {
    const data = await apiCall("/wallets");
    const select = document.getElementById("send-wallet");

    select.innerHTML = '<option value="">Select a wallet...</option>';
    data.wallets.forEach((wallet) => {
      const option = document.createElement("option");
      option.value = wallet.wallet_id;
      option.textContent = `${wallet.name} (${wallet.wallet_type})`;
      option.dataset.address = wallet.address;
      option.dataset.type = wallet.wallet_type;
      select.appendChild(option);
    });
  } catch (error) {
    showToast("Failed to load wallets", "error");
  }
}

async function updateWalletBalance() {
  const select = document.getElementById("send-wallet");
  const walletId = select.value;
  const infoEl = document.getElementById("wallet-balance-info");

  if (!walletId) {
    infoEl.textContent = "";
    return;
  }

  try {
    const balance = await apiCall(`/wallet/${walletId}/balance`);
    infoEl.textContent = `Balance: ${balance.total_sats.toLocaleString()} sats (${balance.total_btc} BTC)`;
    infoEl.classList.remove("text-gray-500");
    infoEl.classList.add("text-green-600");
  } catch (error) {
    infoEl.textContent = "Failed to load balance";
    infoEl.classList.remove("text-green-600");
    infoEl.classList.add("text-red-500");
  }
}

async function sendBitcoin(event) {
  event.preventDefault();

  const walletId = document.getElementById("send-wallet").value;
  const to = document.getElementById("send-to").value;
  const amount = parseInt(document.getElementById("send-amount").value);
  const feeRate = document.getElementById("send-fee").value;

  const select = document.getElementById("send-wallet");
  const walletType = select.options[select.selectedIndex].dataset.type;

  // Show loading
  document.getElementById("send-btn-text").classList.add("hidden");
  document.getElementById("send-btn-loading").classList.remove("hidden");
  document.getElementById("tx-result").classList.add("hidden");

  try {
    // Use correct endpoint based on wallet type (case-insensitive)
    const isTaproot = walletType && walletType.toLowerCase() === "taproot";
    const endpoint = isTaproot
      ? `/wallet/${walletId}/send-taproot`
      : `/wallet/${walletId}/send`;

    const payload = {
      wallet_id: walletId,
      to_address: to,
      amount_sats: amount,
    };

    if (feeRate) {
      payload.fee_rate = parseInt(feeRate);
    }

    const result = await apiCall(endpoint, {
      method: "POST",
      body: JSON.stringify(payload),
    });

    // Show result
    document.getElementById("tx-id").textContent = result.txid;
    document.getElementById("tx-amount").textContent =
      result.amount_sats.toLocaleString();
    document.getElementById("tx-fee").textContent =
      result.fee_sats.toLocaleString();
    document.getElementById("tx-result").classList.remove("hidden");

    showToast("Transaction sent successfully!", "success");

    // Clear form
    document.getElementById("send-to").value = "";
    document.getElementById("send-amount").value = "";
    document.getElementById("send-fee").value = "";

    // Refresh dashboard
    loadDashboard();
  } catch (error) {
    showToast(`Transaction failed: ${error.message}`, "error");
  } finally {
    document.getElementById("send-btn-text").classList.remove("hidden");
    document.getElementById("send-btn-loading").classList.add("hidden");
  }
}

// Settings
async function loadSystemInfo() {
  try {
    const info = await apiCall("/info");

    document.getElementById("system-info").innerHTML = `
            <p class="text-gray-700"><span class="font-medium">Threshold:</span> ${info.threshold}-of-${info.parties}</p>
            <p class="text-gray-700"><span class="font-medium">Wallets:</span> ${info.wallets_count}</p>
            <p class="text-gray-700 font-medium mt-2">Nodes:</p>
            <ul class="list-disc list-inside ml-4">
                ${info.nodes.map((node) => `<li class="text-gray-600">${node}</li>`).join("")}
            </ul>
        `;

    // Also load node status
    await loadNodeStatus();
  } catch (error) {
    document.getElementById("system-info").innerHTML =
      '<p class="text-red-500">Failed to load system info. Check coordinator connection.</p>';
  }
}

async function loadNodeStatus() {
  try {
    const status = await apiCall("/nodes/status");

    const statusEl = document.getElementById("node-status");

    const allReady = status.all_cggmp24_ready || false;
    const allReachable = status.all_reachable || false;

    if (allReady && allReachable) {
      statusEl.innerHTML = `
                <p class="text-green-700 font-medium">✓ All nodes are initialized and ready!</p>
                <p class="text-sm text-gray-600 mt-1">You can create CGGMP24 wallets.</p>
            `;
    } else if (!allReachable) {
      statusEl.innerHTML = `
                <p class="text-red-600 font-medium">✗ Some nodes are unreachable</p>
                <p class="text-sm text-gray-600 mt-1">Make sure all Docker nodes are running.</p>
            `;
    } else {
      const nodes = status.nodes || [];
      const readyCount = nodes.filter((n) => n.cggmp24_ready).length;
      statusEl.innerHTML = `
                <p class="text-yellow-700 font-medium">⚠ Nodes not initialized</p>
                <p class="text-sm text-gray-600 mt-1">Ready: ${readyCount}/${nodes.length} nodes</p>
                <p class="text-sm text-gray-600">Click the button below to initialize.</p>
            `;
    }
  } catch (error) {
    const statusEl = document.getElementById("node-status");
    statusEl.innerHTML = `
            <p class="text-red-600 font-medium">✗ Cannot check node status</p>
            <p class="text-sm text-gray-600 mt-1">Make sure nodes are running.</p>
        `;
  }
}

async function initializeCggmp24() {
  // Show loading
  document.getElementById("init-btn-text").classList.add("hidden");
  document.getElementById("init-btn-loading").classList.remove("hidden");

  try {
    const result = await apiCall("/aux-info/start", {
      method: "POST",
      body: JSON.stringify({}),
    });

    showToast(
      "Initialization started! This may take a few minutes...",
      "success",
    );

    // Poll for completion
    let attempts = 0;
    const maxAttempts = 120; // 10 minutes max

    const pollInterval = setInterval(async () => {
      attempts++;

      try {
        const status = await apiCall("/nodes/status");

        if (status.all_cggmp24_ready) {
          clearInterval(pollInterval);
          showToast("CGGMP24 nodes initialized successfully!", "success");
          await loadNodeStatus();
          document.getElementById("init-btn-text").classList.remove("hidden");
          document.getElementById("init-btn-loading").classList.add("hidden");
        }

        if (attempts >= maxAttempts) {
          clearInterval(pollInterval);
          showToast("Initialization timeout - check status manually", "error");
          await loadNodeStatus();
          document.getElementById("init-btn-text").classList.remove("hidden");
          document.getElementById("init-btn-loading").classList.add("hidden");
        }
      } catch (err) {
        // Continue polling
      }
    }, 5000); // Check every 5 seconds
  } catch (error) {
    showToast(`Initialization failed: ${error.message}`, "error");
    document.getElementById("init-btn-text").classList.remove("hidden");
    document.getElementById("init-btn-loading").classList.add("hidden");
  }
}

function saveSettings() {
  const url = document.getElementById("coordinator-url").value;
  coordinatorUrl = url;
  localStorage.setItem("coordinatorUrl", url);
  showToast("Settings saved!", "success");
  loadSystemInfo();
}

// Mining (Regtest only)
async function loadMiningWallets() {
  try {
    const data = await apiCall("/wallets");
    const select = document.getElementById("mine-wallet");

    if (!select) return;

    select.innerHTML =
      '<option value="">Select wallet for mining rewards...</option>';
    data.wallets.forEach((wallet) => {
      const option = document.createElement("option");
      option.value = wallet.wallet_id;
      option.textContent = `${wallet.name} (${wallet.address.substring(0, 20)}...)`;
      select.appendChild(option);
    });
  } catch (error) {
    console.error("Failed to load wallets for mining:", error);
  }
}

async function mineBlocks(blocks) {
  const walletId = document.getElementById("mine-wallet").value;
  const customBlocks = document.getElementById("mine-blocks").value;
  const resultEl = document.getElementById("mine-result");

  // Use custom blocks if provided and different from button default
  const numBlocks = blocks || parseInt(customBlocks) || 1;

  if (!walletId) {
    showToast("Please select a wallet to receive mining rewards", "error");
    return;
  }

  resultEl.classList.add("hidden");
  showToast(`Mining ${numBlocks} block(s)...`, "info");

  try {
    const result = await apiCall(`/wallet/${walletId}/mine`, {
      method: "POST",
      body: JSON.stringify({ blocks: numBlocks }),
    });

    resultEl.innerHTML = `<p class="text-sm text-green-700">✓ Mined ${result.blocks_mined} block(s) to ${result.address}</p>`;
    resultEl.classList.remove("hidden");

    showToast(`Successfully mined ${result.blocks_mined} block(s)!`, "success");

    // Refresh dashboard to show updated balances
    loadDashboard();
  } catch (error) {
    resultEl.innerHTML = `<p class="text-sm text-red-600">✗ Mining failed: ${error.message}</p>`;
    resultEl.classList.remove("hidden");
    showToast(`Mining failed: ${error.message}`, "error");
  }
}

// Toast notifications
function showToast(message, type = "info") {
  const toast = document.getElementById("toast");
  const toastMessage = document.getElementById("toast-message");

  toastMessage.textContent = message;
  toast.classList.remove("hidden");

  setTimeout(() => {
    toast.classList.add("hidden");
  }, 3000);
}
