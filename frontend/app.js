// app.js — Logique client UltraHDR Studio

let currentMode = 'split';
let isDraggingSplit = false;
let splitPosition = 50.0; // En pourcentage
let zoomLevel = 1.0;
let isPanning = false;
let startPanX = 0, startPanY = 0;
let currentPanX = 0, currentPanY = 0;
let updateDebounceTimer = null;
let currentAnalysis = null;

// Éléments du DOM
const dropzone = document.getElementById('dropzone');
const viewerContainer = document.getElementById('viewer-container');
const fileInput = document.getElementById('file-input');
const btnOpenFile = document.getElementById('btn-open-file');
const btnExportMain = document.getElementById('btn-export-main');
const btnExportAside = document.getElementById('btn-export-aside');

const splitWrapper = document.getElementById('split-view-wrapper');
const splitBeforeLayer = document.getElementById('split-before-layer');
const splitHandle = document.getElementById('split-handle');
const sideWrapper = document.getElementById('side-by-side-wrapper');
const singleWrapper = document.getElementById('single-view-wrapper');

const imgBefore = document.getElementById('img-before');
const imgAfter = document.getElementById('img-after');
const imgSideBefore = document.getElementById('img-side-before');
const imgSideAfter = document.getElementById('img-side-after');
const imgSingle = document.getElementById('img-single');

const displayBadge = document.getElementById('display-badge');
const displayStatusText = document.getElementById('display-status-text');
const hdrShortcutAlert = document.getElementById('hdr-shortcut-alert');
const hdrShortcutText = document.getElementById('hdr-shortcut-text');

// ── 1. Initialisation & Détection Matérielle ──

async function initDisplayStatus() {
    try {
        const res = await fetch('/api/display-info');
        const info = await res.json();

        displayBadge.className = 'badge-status';
        if (info.has_hdr_enabled) {
            displayBadge.classList.add('hdr-enabled');
            displayStatusText.textContent = 'Écran HDR Actif';
            hdrShortcutAlert.style.display = 'none';
        } else {
            displayBadge.classList.add('hdr-disabled');
            displayStatusText.textContent = 'Écran SDR (Simulation Active)';
            if (info.shortcut_tip) {
                hdrShortcutText.innerHTML = `Votre écran est compatible HDR matériel ! <strong>Win + Alt + B</strong> permet d'activer le HDR dans Windows.`;
                hdrShortcutAlert.style.display = 'flex';
            }
        }
    } catch (e) {
        console.warn('Erreur détection écran:', e);
        displayBadge.className = 'badge-status hdr-disabled';
        displayStatusText.textContent = 'Mode Simulation SDR';
    }
}

// ── 2. Gestion Drag & Drop et Chargement Fichier ──

btnOpenFile.addEventListener('click', () => fileInput.click());

['dragenter', 'dragover'].forEach(name => {
    document.addEventListener(name, (e) => {
        e.preventDefault();
        dropzone.querySelector('.dropzone-box')?.classList.add('drag-over');
    });
});

['dragleave', 'drop'].forEach(name => {
    document.addEventListener(name, (e) => {
        e.preventDefault();
        dropzone.querySelector('.dropzone-box')?.classList.remove('drag-over');
    });
});

document.addEventListener('drop', (e) => {
    e.preventDefault();
    if (e.dataTransfer && e.dataTransfer.files.length > 0) {
        uploadFile(e.dataTransfer.files[0]);
    }
});

fileInput.addEventListener('change', (e) => {
    if (e.target.files.length > 0) {
        uploadFile(e.target.files[0]);
    }
});

async function uploadFile(file) {
    const formData = new FormData();
    formData.append('file', file);

    displayStatusText.textContent = 'Chargement et analyse...';

    try {
        const res = await fetch('/api/upload', {
            method: 'POST',
            body: formData
        });

        if (!res.ok) {
            const err = await res.json();
            alert('Erreur: ' + (err.detail || 'Impossible de charger l\'image'));
            return;
        }

        const data = await res.json();
        setupImageSession(data);
    } catch (e) {
        alert('Erreur lors du transfert du fichier: ' + e);
    } finally {
        initDisplayStatus();
    }
}

function setupImageSession(data) {
    // 1. Afficher les infos du fichier dans la barre supérieure
    document.getElementById('file-info-bar').style.display = 'flex';
    document.getElementById('info-filename').textContent = data.filename;
    document.getElementById('info-format').textContent = data.format;
    document.getElementById('info-res').textContent = `${data.width}×${data.height}`;
    document.getElementById('info-stops').textContent = `+${data.gain_max.toFixed(1)} IL Boost`;

    // 2. Remplir la carte de proposition
    currentAnalysis = data.analysis;
    const a = data.analysis;
    document.getElementById('proposal-score').textContent = `${a.potential_score}%`;
    document.getElementById('proposal-title').textContent = a.category_title;
    document.getElementById('proposal-text').textContent = a.explanation;
    document.getElementById('meta-highlights').textContent = `${a.highlight_ratio_pct}%`;
    document.getElementById('meta-contrast').textContent = `${a.contrast_ratio}:1`;

    // 3. Remplir les curseurs avec les paramètres actuels
    const p = data.current_params;
    setSliderValues(p);

    // 4. Activer les boutons d'export
    btnExportMain.disabled = false;
    btnExportAside.disabled = false;

    // 5. Basculer de la dropzone à la visionneuse
    dropzone.style.display = 'none';
    viewerContainer.style.display = 'flex';

    // 6. Charger les flux d'images
    reloadImages();
    resetZoomAndPan();
}

function setSliderValues(p) {
    document.getElementById('slider-peak-nits').value = p.peak_nits;
    document.getElementById('val-peak-nits').textContent = `${Math.round(p.peak_nits)} nits`;

    document.getElementById('slider-gamma').value = p.hi_gamma;
    document.getElementById('val-gamma').textContent = p.hi_gamma.toFixed(2);

    document.getElementById('slider-shadow').value = p.shadow_boost;
    document.getElementById('val-shadow').textContent = `${p.shadow_boost.toFixed(2)}×`;

    document.getElementById('slider-threshold').value = p.highlight_threshold;
    document.getElementById('val-threshold').textContent = p.highlight_threshold.toFixed(2);

    document.getElementById('slider-ev').value = p.ev_shift || 0.0;
    document.getElementById('val-ev').textContent = `${(p.ev_shift || 0.0) >= 0 ? '+' : ''}${(p.ev_shift || 0.0).toFixed(1)} EV`;
}

function reloadImages() {
    const ts = Date.now();
    const urlOrig = `/api/image/original_sdr?t=${ts}`;
    const urlHdrSim = `/api/image/hdr_simulated?t=${ts}`;
    const urlGainMap = `/api/image/gain_map?t=${ts}`;
    const urlHeat = `/api/image/heatmap?t=${ts}`;
    const urlUltraHdr = `/api/image/ultrahdr?t=${ts}`;

    imgBefore.src = urlOrig;
    imgAfter.src = urlHdrSim;

    imgSideBefore.src = urlOrig;
    imgSideAfter.src = urlHdrSim;

    if (currentMode === 'gain_map') {
        imgSingle.src = urlGainMap;
    } else if (currentMode === 'heatmap') {
        imgSingle.src = urlHeat;
    } else if (currentMode === 'true_ultrahdr') {
        imgSingle.src = urlUltraHdr;
    }
}

// ── 3. Glissière Avant / Après (Split-Screen) ──

function updateSplitPosition(pct) {
    splitPosition = Math.max(0, Math.min(100, pct));
    splitHandle.style.left = `${splitPosition}%`;
    splitBeforeLayer.style.clipPath = `polygon(0 0, ${splitPosition}% 0, ${splitPosition}% 100%, 0 100%)`;
}

function onSplitDrag(e) {
    if (!isDraggingSplit) return;
    const rect = splitWrapper.getBoundingClientRect();
    const clientX = e.touches ? e.touches[0].clientX : e.clientX;
    const pct = ((clientX - rect.left) / rect.width) * 100;
    updateSplitPosition(pct);
}

splitHandle.addEventListener('mousedown', (e) => {
    isDraggingSplit = true;
    e.preventDefault();
});

splitWrapper.addEventListener('mousedown', (e) => {
    // Si on clique sur la zone, déplacer la glissière directement
    const rect = splitWrapper.getBoundingClientRect();
    const pct = ((e.clientX - rect.left) / rect.width) * 100;
    updateSplitPosition(pct);
    isDraggingSplit = true;
});

window.addEventListener('mousemove', onSplitDrag);
window.addEventListener('mouseup', () => isDraggingSplit = false);

splitHandle.addEventListener('touchstart', () => isDraggingSplit = true);
window.addEventListener('touchmove', onSplitDrag);
window.addEventListener('touchend', () => isDraggingSplit = false);

// ── 4. Sélecteur de Mode d'Affichage ──

document.querySelectorAll('.mode-btn').forEach(btn => {
    btn.addEventListener('click', () => {
        document.querySelectorAll('.mode-btn').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        setViewerMode(btn.dataset.mode);
    });
});

function setViewerMode(mode) {
    currentMode = mode;
    splitWrapper.style.display = 'none';
    sideWrapper.style.display = 'none';
    singleWrapper.style.display = 'none';
    document.getElementById('heatmap-legend').style.display = 'none';

    if (mode === 'split') {
        splitWrapper.style.display = 'flex';
        updateSplitPosition(splitPosition);
    } else if (mode === 'side_by_side') {
        sideWrapper.style.display = 'flex';
    } else if (mode === 'true_ultrahdr') {
        singleWrapper.style.display = 'flex';
        imgSingle.src = `/api/image/ultrahdr?t=${Date.now()}`;
    } else if (mode === 'gain_map') {
        singleWrapper.style.display = 'flex';
        imgSingle.src = `/api/image/gain_map?t=${Date.now()}`;
    } else if (mode === 'heatmap') {
        singleWrapper.style.display = 'flex';
        document.getElementById('heatmap-legend').style.display = 'flex';
        const peakNits = document.getElementById('slider-peak-nits').value;
        document.getElementById('legend-max-nits').textContent = `${peakNits}+ nits`;
        imgSingle.src = `/api/image/heatmap?t=${Date.now()}`;
    }
}

// ── 5. Réglages en Temps Réel et Debounce ──

function getFormParameters() {
    return {
        peak_nits: parseFloat(document.getElementById('slider-peak-nits').value),
        sdr_white: 203.0,
        hi_gamma: parseFloat(document.getElementById('slider-gamma').value),
        shadow_boost: parseFloat(document.getElementById('slider-shadow').value),
        highlight_threshold: parseFloat(document.getElementById('slider-threshold').value),
        ev_shift: parseFloat(document.getElementById('slider-ev').value),
        tone_map_mode: document.getElementById('select-tonemap').value
    };
}

function triggerParamUpdate() {
    clearTimeout(updateDebounceTimer);
    updateDebounceTimer = setTimeout(async () => {
        const params = getFormParameters();
        try {
            const res = await fetch('/api/update-params', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(params)
            });
            if (res.ok) {
                const data = await res.json();
                document.getElementById('stat-gain-min').textContent = `${data.gain_min.toFixed(2)} IL`;
                document.getElementById('stat-gain-max').textContent = `+${data.gain_max.toFixed(2)} IL`;
                document.getElementById('stat-multiplier').textContent = `${data.peak_boost_multiplier}×`;
                document.getElementById('info-stops').textContent = `+${data.gain_max.toFixed(1)} IL Boost`;
                reloadImages();
            }
        } catch (e) {
            console.error('Erreur mise à jour:', e);
        }
    }, 60);
}

// Liaisons curseurs
document.getElementById('slider-peak-nits').addEventListener('input', (e) => {
    document.getElementById('val-peak-nits').textContent = `${Math.round(e.target.value)} nits`;
    triggerParamUpdate();
});

document.getElementById('slider-gamma').addEventListener('input', (e) => {
    document.getElementById('val-gamma').textContent = parseFloat(e.target.value).toFixed(2);
    triggerParamUpdate();
});

document.getElementById('slider-shadow').addEventListener('input', (e) => {
    document.getElementById('val-shadow').textContent = `${parseFloat(e.target.value).toFixed(2)}×`;
    triggerParamUpdate();
});

document.getElementById('slider-threshold').addEventListener('input', (e) => {
    document.getElementById('val-threshold').textContent = parseFloat(e.target.value).toFixed(2);
    triggerParamUpdate();
});

document.getElementById('slider-ev').addEventListener('input', (e) => {
    const val = parseFloat(e.target.value);
    document.getElementById('val-ev').textContent = `${val >= 0 ? '+' : ''}${val.toFixed(1)} EV`;
    triggerParamUpdate();
});

document.getElementById('select-tonemap').addEventListener('change', () => triggerParamUpdate());

// Qualités d'exportation
document.getElementById('slider-quality').addEventListener('input', (e) => {
    document.getElementById('val-quality').textContent = `${e.target.value}%`;
});
document.getElementById('slider-gm-quality').addEventListener('input', (e) => {
    document.getElementById('val-gm-quality').textContent = `${e.target.value}%`;
});

// ── 6. Proposition Automatique & Profils Prédéfinis ──

document.getElementById('btn-apply-proposal').addEventListener('click', () => {
    if (!currentAnalysis || !currentAnalysis.recommended_preset) return;
    const p = currentAnalysis.recommended_preset;
    setSliderValues({
        peak_nits: p.peak_nits,
        hi_gamma: p.hi_gamma,
        shadow_boost: p.shadow_boost,
        highlight_threshold: p.highlight_threshold,
        ev_shift: 0.0
    });
    triggerParamUpdate();
});

document.getElementById('select-preset').addEventListener('change', (e) => {
    if (!currentAnalysis || !currentAnalysis.available_presets) return;
    const found = currentAnalysis.available_presets.find(pr => pr.name === e.target.value);
    if (found) {
        setSliderValues({
            peak_nits: found.peak_nits,
            hi_gamma: found.hi_gamma,
            shadow_boost: found.shadow_boost,
            highlight_threshold: found.highlight_threshold,
            ev_shift: 0.0
        });
        triggerParamUpdate();
    }
});

// ── 7. Contrôles de Zoom et Centrage ──

function applyZoom() {
    const transformStr = `scale(${zoomLevel}) translate(${currentPanX}px, ${currentPanY}px)`;
    [imgBefore, imgAfter, imgSingle, imgSideBefore, imgSideAfter].forEach(el => {
        if (el) el.style.transform = transformStr;
    });
    document.getElementById('zoom-level').textContent = `${Math.round(zoomLevel * 100)}%`;
}

function resetZoomAndPan() {
    zoomLevel = 1.0;
    currentPanX = 0;
    currentPanY = 0;
    applyZoom();
}

document.getElementById('btn-zoom-fit').addEventListener('click', resetZoomAndPan);
document.getElementById('btn-zoom-100').addEventListener('click', () => {
    zoomLevel = 1.0;
    currentPanX = 0;
    currentPanY = 0;
    applyZoom();
});

document.getElementById('btn-zoom-in').addEventListener('click', () => {
    zoomLevel = Math.min(zoomLevel * 1.25, 5.0);
    applyZoom();
});

document.getElementById('btn-zoom-out').addEventListener('click', () => {
    zoomLevel = Math.max(zoomLevel / 1.25, 0.25);
    applyZoom();
});

// ── 8. Exportation JPEG Ultra HDR ──

async function executeExport() {
    const btn = btnExportMain;
    btn.disabled = true;
    btn.textContent = 'Génération Ultra HDR...';

    const payload = {
        base_quality: parseInt(document.getElementById('slider-quality').value),
        gainmap_quality: parseInt(document.getElementById('slider-gm-quality').value),
        gainmap_downscale: 1.0,
        save_standalone_gainmap: false
    };

    try {
        const res = await fetch('/api/export', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });

        if (!res.ok) {
            alert('Erreur lors de l\'exportation');
            return;
        }

        const blob = await res.blob();
        const disposition = res.headers.get('Content-Disposition');
        let filename = 'photo_UltraHDR.jpg';
        if (disposition && disposition.includes('filename=')) {
            filename = disposition.split('filename=')[1].replace(/"/g, '').trim();
        }

        const downloadUrl = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = downloadUrl;
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(downloadUrl);
    } catch (e) {
        alert('Erreur lors de l\'exportation: ' + e);
    } finally {
        btn.disabled = false;
        btn.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg> Exporter JPEG Ultra HDR`;
    }
}

btnExportMain.addEventListener('click', executeExport);
btnExportAside.addEventListener('click', executeExport);

// ── 9. Gestion des Accordéons (Panneaux Rétractables) ──
document.querySelectorAll('.card-header.accordion-toggle').forEach(header => {
    header.addEventListener('click', (e) => {
        // Ne pas déclencher si on clique sur un bouton ou sélecteur à l'intérieur
        if (e.target.closest('button') || e.target.closest('select')) return;
        const card = header.closest('.control-card');
        if (card) {
            card.classList.toggle('collapsed');
        }
    });
});

const btnExpandAll = document.getElementById('btn-expand-all');
const btnCollapseAll = document.getElementById('btn-collapse-all');

if (btnExpandAll) {
    btnExpandAll.addEventListener('click', () => {
        document.querySelectorAll('.control-card').forEach(c => c.classList.remove('collapsed'));
    });
}

if (btnCollapseAll) {
    btnCollapseAll.addEventListener('click', () => {
        document.querySelectorAll('.control-card').forEach(c => c.classList.add('collapsed'));
    });
}

// Lancement de la détection au démarrage
initDisplayStatus();
