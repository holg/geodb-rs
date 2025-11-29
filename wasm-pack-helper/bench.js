// Store per-benchmark stats so we can compare runs
const benchmarkStats = {}; // label -> { avgLatency, searchTime, startupTime }
const NUM_QUERIES = 2000;  // change to 5000 if you really want max stress
let isRunning = false;

// Helper to log to UI
function log(msg, type = "") {
    const el = document.getElementById("log");
    const div = document.createElement("div");
    div.className = `entry ${type}`;
    div.innerHTML = msg;
    el.appendChild(div);
    el.scrollTop = el.scrollHeight;
    console.log(msg.replace(/<[^>]+>/g, ""));
}

function logSeparator(label) {
    log("<hr>", "info");
    log(`<b>▶ ${label}</b> — new run`, "info");
}

function recordAndCompare(label, avgLatency, searchTime, startupTime) {
    benchmarkStats[label] = {avgLatency, searchTime, startupTime};

    const entries = Object.entries(benchmarkStats);
    if (entries.length === 0) {
        return;
    }

    // Find fastest by avgLatency
    const [fastestLabel, fastest] = entries.reduce((best, current) =>
        current[1].avgLatency < best[1].avgLatency ? current : best
    );

    if (label === fastestLabel && entries.length > 1) {
        log(
            `🏅 <b>${label}</b> is currently the fastest variant with <b>${avgLatency.toFixed(
                4
            )} ms</b> per query.`,
            "success"
        );
    } else if (label !== fastestLabel) {
        const factor = avgLatency / fastest.avgLatency;
        log(
            `⚖️ Compared to fastest (<b>${fastestLabel}</b> at <b>${fastest.avgLatency.toFixed(
                4
            )} ms</b>), <b>${label}</b> is <b>${factor.toFixed(
                2
            )}x</b> slower.`,
            factor <= 1.1 ? "success" : "error"
        );
    }
}

window.runBench = async function (jsPath, label) {
    if (isRunning) {
        log(
            `⏳ A benchmark is already running. Please wait for it to finish before starting <b>${label}</b>.`,
            "info"
        );
        return;
    }
    isRunning = true;

    logSeparator(label);
    log(`Starting <b>${label}</b> benchmark...`, "info");
    log(`Fetching JS module from: <code>${jsPath}</code>`);

    try {
        const t0 = performance.now();

        // Dynamic Import of the Bindings
        const module = await import(jsPath);
        const init = module.default;

        // Construct the path to the .wasm file (Trunk naming: file.js -> file_bg.wasm)
        const wasmPath = jsPath.replace(".js", "_bg.wasm");
        log(`Fetching WASM from: <code>${wasmPath}</code>`);

        // Initialize WASM
        await init(wasmPath);

        // Trigger DB Load (Deserialize)
        if (module.start) {
            module.start();
        } else {
            log(
                "Warning: module.start() not found, assuming auto-start...",
                "error"
            );
        }

        const t1 = performance.now();
        const startupTime = t1 - t0;

        // Verify Data
        const count = module.get_country_count();
        log(
            `✅ <b>Startup Complete:</b> ${startupTime.toFixed(2)} ms`,
            "success"
        );
        log(`Loaded <b>${count}</b> countries.`);

        // Warmup Search
        module.smart_search("Berlin");

        // Stress Test
        log(`Running ${NUM_QUERIES.toLocaleString()} search queries...`);
        const queries = [
            "United",
            "Berlin",
            "Tokyo",
            "+1",
            "California",
            "Zürich",
            "Munich",
            "CN",
            "100",
            "Asia"
        ];

        const tSearchStart = performance.now();
        let hits = 0;
        for (let i = 0; i < NUM_QUERIES; i++) {
            const q = queries[i % queries.length];
            const res = module.smart_search(q);
            hits += res.length;
        }
        const tSearchEnd = performance.now();

        const searchTime = tSearchEnd - tSearchStart;
        const avg = searchTime / NUM_QUERIES;

        log(`<b>Search Time:</b> ${searchTime.toFixed(2)} ms`);
        log(
            `<b>Avg Latency:</b> ${avg.toFixed(4)} ms / query`,
            avg < 0.1 ? "success" : "error"
        );
        log(`Total Hits: ${hits}`);
        log(
            `<br><i>Check Chrome DevTools Memory tab for RAM usage.</i>`,
            "info"
        );

        // Store stats and show relative “x slower/faster” info
        recordAndCompare(label, avg, searchTime, startupTime);
    } catch (e) {
        log(`❌ Fatal Error: ${e.message}`, "error");
        console.error(e);
    } finally {
        isRunning = false;
    }
};
    

