"""backend/server.py — Serveur FastAPI et API REST pour l'éditeur Ultra HDR."""
from __future__ import annotations

import io
import os
import shutil
import tempfile
from pathlib import Path
from typing import Optional, Dict, Any

import cv2
import numpy as np
from PIL import Image
from fastapi import FastAPI, UploadFile, File, Form, HTTPException, Response
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse, FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from core.display_detector import get_display_hdr_status
from core.image_loader import load_image, LoadedImage
from core.analyzer import analyze_image_potential, AnalysisResult
from core.ultrahdr_engine import (
    compute_hdr_and_gain_map, assemble_ultra_hdr_jpeg,
    simulate_hdr_on_sdr, generate_nits_heatmap, encode_gain_map_jpeg
)

app = FastAPI(title="UltraHDR Studio API", version="1.0.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


class ProcessingParams(BaseModel):
    peak_nits: float = 1000.0
    sdr_white: float = 203.0
    shadow_boost: float = 1.15
    hi_gamma: float = 1.0
    highlight_threshold: float = 0.5
    ev_shift: float = 0.0
    tone_map_mode: str = "aces"


class ExportParams(BaseModel):
    base_quality: int = 94
    gainmap_quality: int = 88
    gainmap_downscale: float = 1.0
    save_standalone_gainmap: bool = False
    custom_output_name: Optional[str] = None


class AppState:
    def __init__(self):
        self.loaded_image: Optional[LoadedImage] = None
        self.analysis: Optional[AnalysisResult] = None
        self.params: ProcessingParams = ProcessingParams()

        # Caches pour l'affichage interactif rapide
        self.preview_sdr_uint8: Optional[np.ndarray] = None
        self.preview_lin: Optional[np.ndarray] = None
        self.preview_hdr_nits: Optional[np.ndarray] = None
        self.preview_gain_log2: Optional[np.ndarray] = None
        self.gain_min: float = 0.0
        self.gain_max: float = 0.0

        # Buffers d'images encodées pour l'interface
        self.buf_original_sdr: Optional[bytes] = None
        self.buf_simulated_hdr: Optional[bytes] = None
        self.buf_gain_map: Optional[bytes] = None
        self.buf_heatmap: Optional[bytes] = None
        self.buf_ultrahdr_preview: Optional[bytes] = None

        self.temp_dir = Path(tempfile.mkdtemp(prefix="ultrahdr_"))

    def cleanup(self):
        if self.temp_dir.exists():
            shutil.rmtree(self.temp_dir, ignore_errors=True)


state = AppState()


def _update_previews():
    """Recalcule toutes les images de prévisualisation selon les paramètres actuels."""
    if state.preview_sdr_uint8 is None:
        return

    p = state.params
    is_native = state.loaded_image.is_native_hdr if state.loaded_image else False

    # Calcul unifié et robuste de la Gain Map (jamais blanche)
    state.preview_hdr_nits, state.preview_gain_log2, state.gain_min, state.gain_max = compute_hdr_and_gain_map(
        sdr_uint8=state.preview_sdr_uint8,
        hdr_lin_input=state.preview_lin if is_native else None,
        is_native_hdr=is_native,
        peak_nits=p.peak_nits,
        sdr_white=p.sdr_white,
        shadow_boost=p.shadow_boost,
        hi_gamma=p.hi_gamma,
        highlight_threshold=p.highlight_threshold
    )

    # 1. Image originale SDR (JPEG preview)
    buf_orig = io.BytesIO()
    Image.fromarray(state.preview_sdr_uint8, 'RGB').save(buf_orig, format='JPEG', quality=90)
    state.buf_original_sdr = buf_orig.getvalue()

    # 2. Simulation HDR sur écran SDR (Tone-mapping + EV shift)
    sim_u8 = simulate_hdr_on_sdr(
        state.preview_hdr_nits,
        sdr_white=p.sdr_white,
        ev_shift=p.ev_shift,
        tone_map_mode=p.tone_map_mode
    )
    buf_sim = io.BytesIO()
    Image.fromarray(sim_u8, 'RGB').save(buf_sim, format='JPEG', quality=92)
    state.buf_simulated_hdr = buf_sim.getvalue()

    # 3. Gain Map en niveaux de gris
    state.buf_gain_map = encode_gain_map_jpeg(
        state.preview_gain_log2, state.gain_min, state.gain_max, quality=85
    )

    # 4. Carte thermique des nits
    heat_u8 = generate_nits_heatmap(state.preview_hdr_nits, p.sdr_white, p.peak_nits)
    buf_heat = io.BytesIO()
    Image.fromarray(heat_u8, 'RGB').save(buf_heat, format='JPEG', quality=90)
    state.buf_heatmap = buf_heat.getvalue()

    # 5. JPEG Ultra HDR de prévisualisation matériel Chromium
    state.buf_ultrahdr_preview = assemble_ultra_hdr_jpeg(
        state.preview_sdr_uint8,
        state.preview_gain_log2,
        state.gain_min,
        state.gain_max,
        p.peak_nits,
        p.sdr_white,
        base_quality=90,
        gainmap_quality=82
    )


@app.get("/api/display-info")
def api_get_display_info():
    return get_display_hdr_status()


@app.post("/api/upload")
async def api_upload_file(file: UploadFile = File(...)):
    dest_path = state.temp_dir / file.filename
    with open(dest_path, "wb") as f:
        shutil.copyfileobj(file.file, f)

    try:
        loaded = load_image(dest_path)
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"Erreur lors du décodage du fichier : {e}")

    state.loaded_image = loaded
    state.analysis = analyze_image_potential(loaded)

    rec = state.analysis.recommended_preset
    state.params.peak_nits = rec.peak_nits
    state.params.sdr_white = rec.sdr_white
    state.params.shadow_boost = rec.shadow_boost
    state.params.hi_gamma = rec.hi_gamma
    state.params.highlight_threshold = rec.highlight_threshold
    state.params.ev_shift = 0.0

    h, w = loaded.sdr_uint8.shape[:2]
    max_dim = 1920
    if max(h, w) > max_dim:
        scale = max_dim / max(h, w)
        new_w, new_h = int(w * scale), int(h * scale)
        state.preview_sdr_uint8 = cv2.resize(loaded.sdr_uint8, (new_w, new_h), interpolation=cv2.INTER_AREA)
        state.preview_lin = cv2.resize(loaded.linear_rgb, (new_w, new_h), interpolation=cv2.INTER_AREA)
    else:
        state.preview_sdr_uint8 = loaded.sdr_uint8.copy()
        state.preview_lin = loaded.linear_rgb.copy()

    _update_previews()

    return {
        "filename": loaded.filename,
        "width": loaded.width,
        "height": loaded.height,
        "format": loaded.format_name,
        "source_bit_depth": loaded.source_bit_depth,
        "is_native_hdr": loaded.is_native_hdr,
        "dynamic_range_stops": loaded.dynamic_range_stops,
        "analysis": state.analysis.to_dict(),
        "current_params": state.params.dict(),
        "gain_min": state.gain_min,
        "gain_max": state.gain_max
    }


@app.post("/api/update-params")
def api_update_params(params: ProcessingParams):
    if state.loaded_image is None:
        raise HTTPException(status_code=400, detail="Aucune image chargée")

    state.params = params
    _update_previews()

    return {
        "status": "ok",
        "gain_min": state.gain_min,
        "gain_max": state.gain_max,
        "peak_boost_multiplier": round(2.0 ** state.gain_max, 2),
        "peak_nits": state.params.peak_nits
    }


@app.get("/api/image/{image_type}")
def api_get_image(image_type: str):
    if image_type == "original_sdr":
        data = state.buf_original_sdr
    elif image_type == "hdr_simulated":
        data = state.buf_simulated_hdr
    elif image_type == "gain_map":
        data = state.buf_gain_map
    elif image_type == "heatmap":
        data = state.buf_heatmap
    elif image_type == "ultrahdr":
        data = state.buf_ultrahdr_preview
    else:
        raise HTTPException(status_code=404, detail="Type d'image inconnu")

    if data is None:
        raise HTTPException(status_code=400, detail="Image non disponible")

    return Response(content=data, media_type="image/jpeg")


@app.post("/api/export")
def api_export_image(params: ExportParams):
    if state.loaded_image is None:
        raise HTTPException(status_code=400, detail="Aucune image chargée")

    loaded = state.loaded_image
    p = state.params
    is_native = loaded.is_native_hdr

    # Calcul pleine résolution
    full_hdr_nits, full_gain_log2, gmin, gmax = compute_hdr_and_gain_map(
        sdr_uint8=loaded.sdr_uint8,
        hdr_lin_input=loaded.linear_rgb if is_native else None,
        is_native_hdr=is_native,
        peak_nits=p.peak_nits,
        sdr_white=p.sdr_white,
        shadow_boost=p.shadow_boost,
        hi_gamma=p.hi_gamma,
        highlight_threshold=p.highlight_threshold
    )

    ultra_hdr_bytes = assemble_ultra_hdr_jpeg(
        loaded.sdr_uint8,
        full_gain_log2,
        gmin,
        gmax,
        p.peak_nits,
        p.sdr_white,
        base_quality=params.base_quality,
        gainmap_quality=params.gainmap_quality,
        gainmap_downscale=params.gainmap_downscale
    )

    base_name = Path(loaded.filename).stem
    out_name = params.custom_output_name or f"{base_name}_UltraHDR.jpg"
    if not out_name.lower().endswith(".jpg") and not out_name.lower().endswith(".jpeg"):
        out_name += ".jpg"

    return Response(
        content=ultra_hdr_bytes,
        media_type="image/jpeg",
        headers={"Content-Disposition": f'attachment; filename="{out_name}"'}
    )


frontend_dir = Path(__file__).resolve().parent.parent / "frontend"
if frontend_dir.exists():
    app.mount("/", StaticFiles(directory=str(frontend_dir), html=True), name="frontend")
