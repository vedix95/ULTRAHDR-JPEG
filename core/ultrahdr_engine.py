"""core/ultrahdr_engine.py — Moteur de calcul Ultra HDR, Gain Map et Simulation SDR.

Prend en charge:
1. Calcul précis de la Gain Map selon les spécifications Google / ISO 21496-1.
2. Alignement des données RAW/DNG avec l'image SDR de base pour éviter toute gain map blanche.
3. Encodage et injection des métadonnées Adobe XMP (hdrgm) et CIPA DC-007 MPF APP2.
4. Moteur de simulation de dynamique pour écrans SDR (Tone-mapping ACES, curseur EV, heatmap).
"""
from __future__ import annotations

import io
import math
import struct
from typing import Tuple, Dict, Any, Optional

import numpy as np
from PIL import Image
import cv2

# Matrice de conversion BT.709 vers BT.2020 en lumière linéaire
BT709_TO_BT2020 = np.array([
    [0.6274040, 0.3292820, 0.0433136],
    [0.0690970, 0.9195400, 0.0113612],
    [0.0163916, 0.0880132, 0.8955950],
], dtype=np.float32)

LUM_WEIGHTS = np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)


def srgb_to_linear(x: np.ndarray) -> np.ndarray:
    """Convertit l'espace sRGB non-linéaire vers la lumière linéaire."""
    return np.where(x <= 0.04045, x / 12.92, ((np.maximum(x, 0.0) + 0.055) / 1.055) ** 2.4).astype(np.float32)


def linear_to_srgb(x: np.ndarray) -> np.ndarray:
    """Convertit la lumière linéaire vers l'espace sRGB avec gamma."""
    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * (x ** (1.0 / 2.4)) - 0.055).astype(np.float32)


# ── Moteur Unifié de Calcul HDR & Gain Map ────────────────────────────────────

def compute_hdr_and_gain_map(
    sdr_uint8: np.ndarray,
    hdr_lin_input: Optional[np.ndarray] = None,
    is_native_hdr: bool = False,
    peak_nits: float = 1000.0,
    sdr_white: float = 203.0,
    shadow_boost: float = 1.0,
    hi_gamma: float = 1.0,
    highlight_threshold: float = 0.4,
) -> Tuple[np.ndarray, np.ndarray, float, float]:
    """Calcule l'image HDR (en nits) et la Gain Map (log2) associée à l'image de base SDR.

    Garantit une orientation naturelle et correcte de la Gain Map :
    - Les zones sombres/ombres ont un gain nul (noir = 0 dans la Gain Map).
    - Les zones claires/hautes lumières reçoivent le boost HDR (blanc = 255 dans la Gain Map).
    - Immunisé contre les erreurs de broadcasting de dimensions d'entrée.
    """
    from core.image_loader import normalize_image_array

    # 1. Normalisation stricte en forme (H, W, 3)
    sdr_uint8 = normalize_image_array(sdr_uint8)
    if hdr_lin_input is not None:
        hdr_lin_input = normalize_image_array(hdr_lin_input)
        if hdr_lin_input.shape[:2] != sdr_uint8.shape[:2]:
            hdr_lin_input = cv2.resize(
                hdr_lin_input, (sdr_uint8.shape[1], sdr_uint8.shape[0]), interpolation=cv2.INTER_AREA
            )

    # 2. Base SDR en lumière linéaire
    sdr_lin = srgb_to_linear(sdr_uint8.astype(np.float32) / 255.0)
    L_sdr = np.clip((sdr_lin * LUM_WEIGHTS).sum(axis=-1), 0.0, 1.0)
    peak_boost = max(peak_nits / sdr_white, 1.05)

    # 2. Masque de hautes lumières progressif (smoothstep)
    thresh = float(np.clip(highlight_threshold, 0.05, 0.85))
    t = np.clip((L_sdr - thresh) / max(1.0 - thresh, 1e-4), 0.0, 1.0)
    w_high = t * t * (3.0 - 2.0 * t)

    # 3. Modulation par les données de capteur RAW/DNG (sans division par les ombres)
    if is_native_hdr and hdr_lin_input is not None:
        L_raw = np.clip((hdr_lin_input * LUM_WEIGHTS).sum(axis=-1), 0.0, None)
        raw_headroom = np.maximum(L_raw - L_sdr, 0.0)
        p99_headroom = float(np.percentile(raw_headroom, 99.5))
        if p99_headroom > 1e-3:
            raw_mod = np.clip(raw_headroom / p99_headroom, 0.0, 1.0)
            highlight_weight = (w_high * (0.6 * w_high + 0.4 * raw_mod)) ** hi_gamma
        else:
            highlight_weight = (w_high ** hi_gamma)
    else:
        highlight_weight = (w_high ** hi_gamma)

    # 4. Facteur de boost strictement monotone avec la luminance
    s_boost = max(shadow_boost, 1.0)
    boost = s_boost + (peak_boost - s_boost) * highlight_weight
    boost = np.clip(boost, 1.0, peak_boost)

    # Image HDR en nits
    hdr_nits = (sdr_lin * sdr_white * boost[:, :, None]).astype(np.float32)

    # 5. Calcul de la Gain Map en log2 (Google Ultra HDR standard)
    gain_log2 = np.maximum(np.log2(boost), 0.0).astype(np.float32)

    gain_min = float(np.min(gain_log2))
    gain_max = float(np.max(gain_log2))
    if gain_max < 0.05:
        gain_max = float(np.log2(peak_boost))

    return hdr_nits, gain_log2, gain_min, gain_max


# Rétrocompatibilité avec les fonctions antérieures
def compute_inverse_tone_map(
    sdr_lin: np.ndarray,
    sdr_white: float = 203.0,
    peak_nits: float = 1000.0,
    shadow_boost: float = 1.2,
    hi_gamma: float = 1.0,
    highlight_threshold: float = 0.5,
) -> np.ndarray:
    L = np.clip((sdr_lin * LUM_WEIGHTS).sum(axis=-1, keepdims=True), 1e-6, 1.0)
    peak_boost = peak_nits / sdr_white
    norm_t = np.clip((L - highlight_threshold) / max(1.0 - highlight_threshold, 1e-4), 0.0, 1.0)
    norm_t = norm_t * norm_t * (3.0 - 2.0 * norm_t)
    effective_hi = norm_t ** hi_gamma
    boost = shadow_boost + (peak_boost - shadow_boost) * effective_hi
    return (sdr_lin * sdr_white * boost).astype(np.float32)


def compute_gain_map(
    sdr_lin: np.ndarray,
    hdr_nits: np.ndarray,
    sdr_white: float = 203.0,
) -> Tuple[np.ndarray, float, float]:
    hdr_norm = hdr_nits / sdr_white
    L_sdr = np.clip((sdr_lin * LUM_WEIGHTS).sum(axis=-1), 1e-4, None)
    L_hdr = np.clip((hdr_norm * LUM_WEIGHTS).sum(axis=-1), 1e-4, None)
    gain = np.maximum(np.log2(L_hdr) - np.log2(L_sdr), 0.0)
    return gain.astype(np.float32), 0.0, float(np.max(gain))


# ── XMP et MPF APP2 Google Ultra HDR ─────────────────────────────────────────

def _build_xmp_hdrgm(gain_min: float, gain_max: float, hdr_capacity: float) -> bytes:
    """Segment XMP APP1 conforme Adobe / Google Ultra HDR."""
    xml = (
        '<?xpacket begin="\ufeff" id="W5M0MpCehiHzreSzNTczkc9d"?>\n'
        '<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="XMP Core 5.5.0">\n'
        ' <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">\n'
        '  <rdf:Description rdf:about=""\n'
        '    xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/"\n'
        '   hdrgm:Version="1.0"\n'
        f'   hdrgm:GainMapMin="{gain_min:.6f}"\n'
        f'   hdrgm:GainMapMax="{gain_max:.6f}"\n'
        f'   hdrgm:HDRCapacityMin="0.000000"\n'
        f'   hdrgm:HDRCapacityMax="{hdr_capacity:.6f}"\n'
        '   hdrgm:OffsetHDR="0.000000"\n'
        '   hdrgm:OffsetSDR="0.000000"/>\n'
        ' </rdf:RDF>\n'
        '</x:xmpmeta>\n'
        '<?xpacket end="w"?>'
    )
    payload = b'http://ns.adobe.com/xap/1.0/\x00' + xml.encode('utf-8')
    return b'\xff\xe1' + struct.pack('>H', len(payload) + 2) + payload


def _build_mpf_app2(primary_size: int, gainmap_size: int, gainmap_offset: int) -> bytes:
    """Segment CIPA DC-007 Multi-Picture Format APP2 (90 octets)."""
    TIFF_HDR = b'II' + struct.pack('<H', 42) + struct.pack('<I', 8)
    MP_ENTRIES_TIFF_OFF = 50

    e_ver = struct.pack('<HHI4s', 0xB000, 7, 4, b'0100')
    e_num = struct.pack('<HHII', 0xB001, 4, 1, 2)
    e_entry = struct.pack('<HHII', 0xB002, 7, 32, MP_ENTRIES_TIFF_OFF)
    ifd = struct.pack('<H', 3) + e_ver + e_num + e_entry + struct.pack('<I', 0)

    mp1 = struct.pack('<IIIHH', 0x00030000, primary_size, 0, 0, 0)
    mp2 = struct.pack('<IIIHH', 0x00000000, gainmap_size, gainmap_offset, 0, 0)

    data = b'MPF\x00' + TIFF_HDR + ifd + mp1 + mp2
    assert len(data) == 86
    return b'\xff\xe2' + struct.pack('>H', 88) + data


def encode_gain_map_jpeg(
    gain_log2: np.ndarray,
    gain_min: float,
    gain_max: float,
    quality: int = 85,
    downscale_factor: float = 1.0,
) -> bytes:
    """Quantifie la gain map log2 en JPEG 8-bit niveaux de gris avec contraste dynamique préservé."""
    gain_range = max(gain_max - gain_min, 1e-3)
    gain_u8 = np.clip((gain_log2 - gain_min) / gain_range * 255.0 + 0.5, 0, 255).astype(np.uint8)

    if downscale_factor < 1.0:
        h, w = gain_u8.shape
        new_w = max(int(w * downscale_factor), 16)
        new_h = max(int(h * downscale_factor), 16)
        gain_u8 = cv2.resize(gain_u8, (new_w, new_h), interpolation=cv2.INTER_AREA)

    buf = io.BytesIO()
    Image.fromarray(gain_u8, 'L').save(buf, format='JPEG', quality=quality)
    return buf.getvalue()


def assemble_ultra_hdr_jpeg(
    sdr_uint8: np.ndarray,
    gain_log2: np.ndarray,
    gain_min: float,
    gain_max: float,
    peak_nits: float,
    sdr_white: float,
    base_quality: int = 94,
    gainmap_quality: int = 88,
    gainmap_downscale: float = 1.0,
) -> bytes:
    """Assemble le fichier binaire JPEG Ultra HDR final conforme Google / ISO 21496-1."""
    hdr_capacity = math.log2(max(peak_nits / sdr_white, 1.01))
    xmp_app1 = _build_xmp_hdrgm(gain_min, gain_max, hdr_capacity)

    # 1. Base SDR JPEG
    buf_base = io.BytesIO()
    Image.fromarray(sdr_uint8, 'RGB').save(buf_base, format='JPEG', quality=base_quality, subsampling=0)
    base_jpeg = buf_base.getvalue()

    # 2. Gain Map JPEG avec le même XMP
    gm_raw = encode_gain_map_jpeg(gain_log2, gain_min, gain_max, quality=gainmap_quality, downscale_factor=gainmap_downscale)
    gainmap_jpeg = gm_raw[:2] + xmp_app1 + gm_raw[2:]

    # 3. Offsets et MPF APP2
    A = len(xmp_app1)
    B = len(base_jpeg)
    G = len(gainmap_jpeg)
    primary_size = A + B + 90
    gainmap_offset = B + 80
    mpf_app2 = _build_mpf_app2(primary_size, G, gainmap_offset)

    return b'\xff\xd8' + xmp_app1 + mpf_app2 + base_jpeg[2:] + gainmap_jpeg


# ── Moteur de Simulation pour Écrans SDR ──────────────────────────────────────

def simulate_hdr_on_sdr(
    hdr_nits: np.ndarray,
    sdr_white: float = 203.0,
    ev_shift: float = 0.0,
    tone_map_mode: str = "aces",
) -> np.ndarray:
    """Simule la dynamique HDR sur un écran SDR 8-bit avec compensation d'exposition et tone-mapping."""
    exposure_scale = (2.0 ** ev_shift) / sdr_white
    lin_exposed = hdr_nits * exposure_scale

    if tone_map_mode == "aces":
        x = np.maximum(lin_exposed, 0.0)
        numerator = x * (2.51 * x + 0.03)
        denominator = x * (2.43 * x + 0.59) + 0.14
        tm = np.clip(numerator / np.maximum(denominator, 1e-6), 0.0, 1.0)
        sdr_rgb = linear_to_srgb(tm)
    elif tone_map_mode == "reinhard":
        white_point = max(float(np.percentile(lin_exposed, 99.5)), 1.5)
        numerator = lin_exposed * (1.0 + lin_exposed / (white_point ** 2))
        denominator = 1.0 + lin_exposed
        tm = np.clip(numerator / np.maximum(denominator, 1e-6), 0.0, 1.0)
        sdr_rgb = linear_to_srgb(tm)
    else:
        sdr_rgb = linear_to_srgb(np.clip(lin_exposed, 0.0, 1.0))

    return np.clip(sdr_rgb * 255.0 + 0.5, 0, 255).astype(np.uint8)


def generate_nits_heatmap(
    hdr_nits: np.ndarray,
    sdr_white: float = 203.0,
    peak_nits: float = 1000.0,
) -> np.ndarray:
    """Génère une carte thermique en fausses couleurs visualisant la répartition des nits."""
    lum = (hdr_nits * LUM_WEIGHTS).sum(axis=-1)

    ratio = np.maximum(lum / sdr_white, 1.0)
    stops_above_sdr = np.log2(ratio)
    max_stops = max(np.log2(peak_nits / sdr_white), 1.0)

    norm_heat = np.clip(stops_above_sdr / max_stops, 0.0, 1.0)
    heat_u8 = (norm_heat * 255.0).astype(np.uint8)

    color_map = cv2.applyColorMap(heat_u8, cv2.COLORMAP_TURBO)
    color_rgb = cv2.cvtColor(color_map, cv2.COLOR_BGR2RGB)

    is_sdr = (lum <= sdr_white * 1.05)
    gray = cv2.cvtColor(color_rgb, cv2.COLOR_RGB2GRAY)
    color_rgb[is_sdr] = (np.repeat(gray[is_sdr, None], 3, axis=-1) * 0.5).astype(np.uint8)

    return color_rgb
