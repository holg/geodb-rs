// Global reference to the active WASM module
window.currentModule = null;

// ---------------------------------------------------------
// 1. Result Renderer
// ---------------------------------------------------------
function renderCard(hit) {
    const emoji = hit.emoji || '';
    let kind = hit.kind || "unknown";

    let meta = "";
    if (kind === "city") {
        meta = `${hit.state || "?"}, ${hit.country || "?"}`;
    } else if (kind === "state") {
        meta = `${hit.country || "?"}`;
    } else if (kind === "country") {
        meta = `ISO: ${hit.iso2} ${hit.iso3 ? '/ ' + hit.iso3 : ''} • +${hit.phonecode || "?"}`;
    }

    return `
    <div class="result-card">
        <div class="kind">${kind} <span class="score">Score: ${hit.score || '-'}</span></div>
        <div class="title">${emoji} ${hit.name}</div>
        <div class="meta">${meta}</div>
    </div>`;
}
// ---------------------------------------------------------
// 4. Spatial Handlers (Manual Trigger)
// ---------------------------------------------------------

// Expose to window so HTML buttons can call them
window.runNearest = function() {
    if (!window.currentModule) return;

    const lat = parseFloat(document.getElementById('latInput').value);
    const lng = parseFloat(document.getElementById('lngInput').value);
    const out = document.getElementById('spatialOut');

    if (isNaN(lat) || isNaN(lng)) {
        out.innerHTML = `<div style="color:red; margin-top:10px">Invalid coordinates</div>`;
        return;
    }

    const start = performance.now();
    // Call WASM
    const results = window.currentModule.find_nearest_cities(lat, lng, 5);
    const time = (performance.now() - start).toFixed(2);

    renderSpatialResults(out, results, time, "Nearest Neighbors");
};

window.runRadius = function() {
    if (!window.currentModule) return;

    const lat = parseFloat(document.getElementById('latInput').value);
    const lng = parseFloat(document.getElementById('lngInput').value);
    const out = document.getElementById('spatialOut');

    if (isNaN(lat) || isNaN(lng)) {
        out.innerHTML = `<div style="color:red; margin-top:10px">Invalid coordinates</div>`;
        return;
    }

    const start = performance.now();
    // Call WASM (50km radius hardcoded for demo simplicity, or add input)
    const results = window.currentModule.find_cities_in_radius(lat, lng, 50.0);
    const time = (performance.now() - start).toFixed(2);

    renderSpatialResults(out, results, time, "Radius 50km");
};

function renderSpatialResults(container, results, time, title) {
    if (!results || results.length === 0) {
        container.innerHTML = `<div style="color:#888; margin-top:10px">No cities found. (${time}ms)</div>`;
        return;
    }

    const header = `<div style="font-weight:bold; margin:10px 0 5px 0; color:#444">${title} (${results.length} found in ${time}ms)</div>`;
    const cards = results.slice(0, 10).map(hit => {
        // Reuse renderCard logic, injecting 'city' kind if missing
        hit.kind = 'city';
        // Note: hit object from find_nearest might not have 'score' or 'emoji',
        // but renderCard handles defaults.
        return renderCard(hit);
    }).join('');

    container.innerHTML = header + cards;
}
// ---------------------------------------------------------
// 2. Engine Loader
// ---------------------------------------------------------
window.loadEngine = async function(jsPath, label) {
    const statusEl = document.getElementById('status');
    const appEl = document.getElementById('app');
    const outputIds = ['isoOut', 'countryNameOut', 'stateOut', 'cityOut', 'phoneOut', 'smartOut'];

    // Reset UI
    appEl.style.opacity = "0.5";
    appEl.style.pointerEvents = "none";
    outputIds.forEach(id => document.getElementById(id).innerHTML = '');

    statusEl.className = "stats-box";
    statusEl.innerHTML = `⏳ Loading <b>${label}</b> from <code>${jsPath}</code>...`;

    try {
        const start = performance.now();

        const module = await import(jsPath);
        const wasmPath = jsPath.replace('.js', '_bg.wasm');
        await module.default(wasmPath);

        if (module.start) module.start();

        const time = (performance.now() - start).toFixed(2);
        window.currentModule = module;

        // Get Stats
        let statsHtml = "";
        if (module.get_stats) {
            const s = module.get_stats();
            statsHtml = `Countries: <b>${s.countries}</b> | States: <b>${s.states}</b> | Cities: <b>${s.cities}</b>`;
        }

        let buildHtml = "";
        if (module.get_build_info) {
            const info = module.get_build_info();
            const sizeMB = (info.size_bytes / 1024 / 1024).toFixed(2);
            buildHtml = `<div style="margin-top:8px; font-size:0.85em; color:#555; border-top:1px solid #ccc; padding-top:4px;">
                Binary: <code>${info.filename}</code> (${sizeMB} MB)<br>
                Arch: ${info.architecture} | Blobs: ${info.features.search_blobs}
            </div>`;
        }

        statusEl.innerHTML = `
            <div style="font-size: 1.1em; margin-bottom: 5px;">✅ <b>${label}</b> loaded in ${time}ms</div>
            <div>${statsHtml}</div>
            ${buildHtml}
        `;

        appEl.style.opacity = "1";
        appEl.style.pointerEvents = "auto";

    } catch (err) {
        console.error(err);
        statusEl.className = "stats-box error";
        statusEl.innerHTML = `❌ <b>Error loading ${label}:</b><br>${err.message}`;
    }
};

// ---------------------------------------------------------
// 3. Event Listeners
// ---------------------------------------------------------
document.addEventListener('DOMContentLoaded', () => {

    // Auto-load the fastest engine on page load for a good user experience.
    loadEngine('../flat-blobs/geodb_wasm.js', 'Flat (Flat + Blobs)');

    const bindSearch = (inputId, outputId, searchFn) => {
        document.getElementById(inputId).addEventListener('input', (e) => {
            if (!window.currentModule) return;

            const q = e.target.value.trim();
            const out = document.getElementById(outputId);

            if (q.length < 1) {
                out.innerHTML = '';
                return;
            }

            // Min length check (except for phone)
            if (inputId !== 'phoneInput' && q.length < 2) return;

            const start = performance.now();
            const results = searchFn(q);
            const time = (performance.now() - start).toFixed(2);

            if (!results || results.length === 0) {
                out.innerHTML = `<div style="color:#888; margin-top:10px">No results. (${time}ms)</div>`;
            } else {
                const html = results.slice(0, 10).map(renderCard).join('');
                out.innerHTML = `<div style="text-align:right; font-size:10px; color:#999">${results.length} hits in ${time}ms</div>` + html;
            }
        });
    };

    // 1. ISO Code (Strict)
    bindSearch('isoInput', 'isoOut', (q) => {
        // We use smart_search and filter strictly for ISO codes
        return window.currentModule.smart_search(q).filter(hit => {
            if (hit.kind !== 'country') return false;
            return hit.iso2.toLowerCase() === q.toLowerCase() ||
                (hit.iso3 && hit.iso3.toLowerCase() === q.toLowerCase());
        });
    });

    // 2. Country Name (Substring)
    bindSearch('countryNameInput', 'countryNameOut', (q) => {
        return window.currentModule.smart_search(q).filter(hit => hit.kind === 'country');
    });

    // 3. State Search
    bindSearch('stateInput', 'stateOut', (q) => {
        return window.currentModule.search_state_substring(q);
    });

    // 4. City Search
    bindSearch('cityInput', 'cityOut', (q) => {
        return window.currentModule.search_city_substring(q);
    });

    // 5. Phone Search
    bindSearch('phoneInput', 'phoneOut', (q) => {
        return window.currentModule.search_countries_by_phone(q);
    });

    // 6. Smart Search
    bindSearch('smartInput', 'smartOut', (q) => {
        return window.currentModule.smart_search(q);
    });
});