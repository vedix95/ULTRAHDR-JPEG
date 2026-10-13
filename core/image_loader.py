"""core/image_loader.py — Chargeur universel d'images tous formats.

Prend en charge:
- Camera RAW & DNG (Canon, Nikon, Sony, Fuji, DNG...) via rawpy
- Formats HDR natifs (OpenEXR via OpenEXR officiel, Radiance .HDR, 32-bit Float TIFF)
- Formats mobiles modernes (Apple HEIC, HIF, ProRAW) via Pillow-HEIF et Pillow
- Formats classiques (JPEG, PNG, WebP, TIFF 8/16-bit, BMP, GIF) via Pillow & OpenCV
"""
from __future__ import annotations

import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional, Dict, Any, Tuple

import numpy as np
from PIL import Image

try:
    import pillow_heif
    pillow_heif.register_heif_opener()
    HEIF_AVAILABLE = True
except Exception:
    HEIF_AVAILABLE = False

try:
    import rawpy
    RAW_AVAILABLE = True
except Exception:
    RAW_AVAILABLE = False

try:
    import OpenEXR
    import Imath
    OPENEXR_AVAILABLE = True
except Exception:
    OPENEXR_AVAILABLE = False

import cv2
import imageio.v3 as iio

# Extensions reconnues
RAW_EXTENSIONS = {
    '.dng', '.cr2', '.cr3', '.nef', '.arw', '.rw2', '.raf',
    '.orf', '.pef', '.srw', '.3fr', '.erf', '.kdc', '.mrw',
    '.nrw', '.raw', '.rwl'
}

HDR_EXTENSIONS = {'.exr', '.hdr', '.pfm'}
HEIF_EXTENSIONS = {'.heic', '.heif', '.hif', '.avif'}

LUM_WEIGHTS = np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)


def normalize_image_array(arr: np.ndarray) -> np.ndarray:
    """Normalise n'importe quel tableau NumPy d'image pour garantir la forme (H, W, 3)."""
    if arr is None:
        raise ValueError("Données d'image nulles (None)")

    # 1. Éliminer les dimensions unitaires superflues (ex: (1, H, W, C))
    while arr.ndim > 3 and arr.shape[0] == 1:
        arr = arr[0]
    while arr.ndim > 3 and arr.shape[-1] == 1:
        arr = arr[..., 0]

    # Si toujours > 3 dimensions, prendre la première image/partie
    if arr.ndim > 3:
        arr = arr[0]

    # 2. Cas Channel-first : (C, H, W) avec C in (1, 3, 4) et W > 4
    if arr.ndim == 3 and arr.shape[0] in (1, 3, 4) and arr.shape[2] > 4:
        arr = np.transpose(arr, (1, 2, 0))

    # 3. Cas 2D niveaux de gris : (H, W) -> (H, W, 3)
    if arr.ndim == 2:
        arr = np.repeat(arr[:, :, None], 3, axis=2)

    # 4. Cas 3D : extraction stricte des 3 canaux RGB
    if arr.ndim == 3:
        if arr.shape[2] >= 3:
            arr = arr[:, :, :3]
        elif arr.shape[2] == 1:
            arr = np.repeat(arr, 3, axis=2)
        elif arr.shape[2] == 2:
            arr = np.repeat(arr[:, :, :1], 3, axis=2)

    return arr


@dataclass
class LoadedImage:
    """Représentation unifiée d'une image chargée avec métadonnées de dynamique."""
    # Image de base SDR en uint8 [0..255] (RGB) de forme (H, W, 3)
    sdr_uint8: np.ndarray
    # Image en luminance linéaire flottante (H, W, 3), soit [0..1] pour SDR soit [0..N] pour HDR natif
    linear_rgb: np.ndarray
    width: int
    height: int
    is_native_hdr: bool
    source_bit_depth: int
    format_name: str
    filename: str
    file_size_bytes: int
    exif: Dict[str, Any] = field(default_factory=dict)
    dynamic_range_stops: float = 8.0


def srgb_to_linear(x: np.ndarray) -> np.ndarray:
    """Conversion espace sRGB non-linéaire vers lumière linéaire standard."""
    return np.where(x <= 0.04045, x / 12.92, ((np.maximum(x, 0.0) + 0.055) / 1.055) ** 2.4).astype(np.float32)


def linear_to_srgb(x: np.ndarray) -> np.ndarray:
    """Conversion lumière linéaire standard vers espace sRGB non-linéaire."""
    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * (x ** (1.0 / 2.4)) - 0.055).astype(np.float32)


def aces_tone_map(lin_rgb: np.ndarray) -> np.ndarray:
    """Tone-mapping ACES Filmic standard pour développer le HDR linéaire en SDR harmonieux sans cramage."""
    x = np.maximum(lin_rgb, 0.0)
    numerator = x * (2.51 * x + 0.03)
    denominator = x * (2.43 * x + 0.59) + 0.14
    return np.clip(numerator / np.maximum(denominator, 1e-6), 0.0, 1.0).astype(np.float32)


def read_openexr(path: Path) -> Optional[np.ndarray]:
    """Lit un fichier OpenEXR avec la bibliothèque officielle OpenEXR (32-bit float linéaire)."""
    if not OPENEXR_AVAILABLE:
        return None
    try:
        exr = OpenEXR.InputFile(str(path))
        header = exr.header()
        dw = header['dataWindow']
        w = dw.max.x - dw.min.x + 1
        h = dw.max.y - dw.min.y + 1

        pt = Imath.PixelType(Imath.PixelType.FLOAT)
        avail = list(header['channels'].keys())

        c_map = {}
        for c in ['R', 'G', 'B']:
            if c in avail:
                c_map[c] = c
            elif c.lower() in avail:
                c_map[c] = c.lower()
            else:
                for a in avail:
                    if a.endswith('.' + c) or a.endswith('.' + c.lower()):
                        c_map[c] = a
                        break

        if len(c_map) < 3:
            # Si un seul canal (niveaux de gris Y)
            y_chan = 'Y' if 'Y' in avail else avail[0]
            arr = np.frombuffer(exr.channel(y_chan, pt), dtype=np.float32).reshape(h, w)
            return np.repeat(arr[:, :, None], 3, axis=2)

        channels = [
            np.frombuffer(exr.channel(c_map[c], pt), dtype=np.float32).reshape(h, w)
            for c in ['R', 'G', 'B']
        ]
        return np.stack(channels, axis=-1).astype(np.float32)
    except Exception:
        return None


def load_image(filepath: str | Path) -> LoadedImage:
    """Charge n'importe quel fichier image et extrait ses composantes SDR et dynamique linéaire."""
    path = Path(filepath)
    if not path.exists():
        raise FileNotFoundError(f"Fichier introuvable : {filepath}")

    ext = path.suffix.lower()
    file_size = path.stat().st_size
    filename = path.name

    # ── 1. Fichiers Camera RAW (DNG, CR2, NEF, ARW...) ───────────────────────
    if ext in RAW_EXTENSIONS and RAW_AVAILABLE:
        try:
            with rawpy.imread(str(path)) as raw:
                raw_sdr = raw.postprocess(
                    half_size=False,
                    no_auto_bright=False,
                    output_bps=8,
                    output_color=rawpy.ColorSpace.sRGB,
                    user_flip=0
                )
                raw_16 = raw.postprocess(
                    half_size=False,
                    no_auto_bright=True,
                    output_bps=16,
                    output_color=rawpy.ColorSpace.sRGB,
                    gamma=(1, 1),
                    user_flip=0
                )

                raw_sdr = normalize_image_array(raw_sdr)
                raw_16 = normalize_image_array(raw_16)

                h, w = raw_sdr.shape[:2]
                sdr_lin = srgb_to_linear(raw_sdr.astype(np.float32) / 255.0)
                raw_lin_raw = raw_16.astype(np.float32) / 65535.0

                lum_sdr = (sdr_lin * LUM_WEIGHTS).sum(axis=-1)
                lum_raw = (raw_lin_raw * LUM_WEIGHTS).sum(axis=-1)
                mid_mask = (lum_sdr > 0.15) & (lum_sdr < 0.55)
                if np.any(mid_mask):
                    scale_align = float(np.median(lum_sdr[mid_mask]) / max(np.median(lum_raw[mid_mask]), 1e-4))
                else:
                    scale_align = 1.0

                hdr_aligned = np.maximum(raw_lin_raw * scale_align, sdr_lin)

                p99 = float(np.percentile(hdr_aligned, 99.8))
                p05 = float(np.percentile(hdr_aligned[hdr_aligned > 0], 5.0)) if np.any(hdr_aligned > 0) else 1e-4
                stops = float(np.log2(max(p99 / max(p05, 1e-5), 2.0)))

                return LoadedImage(
                    sdr_uint8=raw_sdr,
                    linear_rgb=hdr_aligned,
                    width=w,
                    height=h,
                    is_native_hdr=True,
                    source_bit_depth=14,
                    format_name=f"Camera RAW ({ext.upper()})",
                    filename=filename,
                    file_size_bytes=file_size,
                    dynamic_range_stops=min(stops, 14.0)
                )
        except Exception:
            pass

    # ── 2. Fichiers HDR Natifs (EXR, HDR, PFM) ─────────────────────────────────
    if ext in HDR_EXTENSIONS:
        hdr_data = None
        # A. Essayer d'abord la bibliothèque officielle OpenEXR (haute précision 32-bit float)
        if ext == '.exr':
            hdr_data = read_openexr(path)

        # B. Tenter OpenCV
        if hdr_data is None:
            try:
                img_cv = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
                if img_cv is not None:
                    if img_cv.ndim == 3 and img_cv.shape[2] >= 3:
                        hdr_data = cv2.cvtColor(img_cv[:, :, :3], cv2.COLOR_BGR2RGB).astype(np.float32)
                    elif img_cv.ndim == 2:
                        hdr_data = cv2.cvtColor(img_cv, cv2.COLOR_GRAY2RGB).astype(np.float32)
            except Exception:
                pass

        # C. Tenter ImageIO
        if hdr_data is None:
            try:
                raw_iio = iio.imread(str(path))
                if raw_iio is not None:
                    hdr_data = normalize_image_array(raw_iio).astype(np.float32)
                    # Si imageio a retourné du uint8 [0..255], normaliser en [0..1]
                    if raw_iio.dtype == np.uint8:
                        hdr_data = hdr_data / 255.0
            except Exception:
                pass

        if hdr_data is not None:
            hdr_data = normalize_image_array(hdr_data)
            hdr_data = np.nan_to_num(hdr_data, nan=0.0, posinf=65500.0, neginf=0.0)
            hdr_data = np.maximum(hdr_data, 0.0)
            h, w = hdr_data.shape[:2]

            # Étalonnage de l'exposition diffuse pour l'image de base SDR
            lum = (hdr_data * LUM_WEIGHTS).sum(axis=-1)
            med_lum = float(np.median(lum))
            # Alignement du gris moyen à 18%
            exposure_scale = 0.18 / max(med_lum, 1e-4)
            exposed_lin = hdr_data * exposure_scale

            # Tone-mapping ACES Filmic pour l'image de base SDR (évite tout cramage)
            sdr_lin = aces_tone_map(exposed_lin)
            sdr_uint8 = np.clip(linear_to_srgb(sdr_lin) * 255.0 + 0.5, 0, 255).astype(np.uint8)

            p99 = float(np.percentile(exposed_lin, 99.8))
            stops = float(np.log2(max(p99 / 0.18, 2.0))) + 8.0

            return LoadedImage(
                sdr_uint8=sdr_uint8,
                linear_rgb=exposed_lin,
                width=w,
                height=h,
                is_native_hdr=True,
                source_bit_depth=32,
                format_name=f"HDR Natif ({ext.upper()})",
                filename=filename,
                file_size_bytes=file_size,
                dynamic_range_stops=min(stops, 20.0)
            )

    # ── 3. Fichiers TIFF 16/32 bits ───────────────────────────────────────────
    if ext in {'.tif', '.tiff'}:
        try:
            img_cv = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
            if img_cv is not None:
                img_cv = normalize_image_array(img_cv)
                h, w = img_cv.shape[:2]
                if img_cv.dtype == np.uint16:
                    rgb_16 = cv2.cvtColor(img_cv, cv2.COLOR_BGR2RGB) if img_cv.ndim == 3 else np.repeat(img_cv[:, :, None], 3, axis=2)
                    lin_float = rgb_16.astype(np.float32) / 65535.0
                    sdr_uint8 = (lin_float * 255.0 + 0.5).astype(np.uint8)
                    return LoadedImage(
                        sdr_uint8=sdr_uint8,
                        linear_rgb=lin_float,
                        width=w,
                        height=h,
                        is_native_hdr=True,
                        source_bit_depth=16,
                        format_name="TIFF 16-bit",
                        filename=filename,
                        file_size_bytes=file_size,
                        dynamic_range_stops=12.0
                    )
        except Exception:
            pass

    # ── 4. Formats Standards (JPEG, PNG, WebP, HEIC, DNG non-bayer) ───────────
    with Image.open(str(path)) as pil_img:
        if pil_img.mode in ('RGBA', 'LA'):
            background = Image.new('RGB', pil_img.size, (255, 255, 255))
            background.paste(pil_img, mask=pil_img.split()[-1])
            rgb_img = background
        elif pil_img.mode != 'RGB':
            rgb_img = pil_img.convert('RGB')
        else:
            rgb_img = pil_img.copy()

        sdr_uint8 = normalize_image_array(np.array(rgb_img, dtype=np.uint8))
        h, w = sdr_uint8.shape[:2]
        lin_rgb = srgb_to_linear(sdr_uint8.astype(np.float32) / 255.0)

        exif_data = {}
        try:
            raw_exif = pil_img.getexif()
            if raw_exif:
                for tag_id, val in raw_exif.items():
                    if isinstance(val, (int, float, str)):
                        exif_data[str(tag_id)] = val
        except Exception:
            pass

        format_label = pil_img.format or ext.replace('.', '').upper()
        if ext in HEIF_EXTENSIONS:
            format_label = f"Apple/HEIF ({ext.upper()})"
        elif ext in RAW_EXTENSIONS:
            format_label = f"Camera RAW / DNG ({ext.upper()})"

        return LoadedImage(
            sdr_uint8=sdr_uint8,
            linear_rgb=lin_rgb,
            width=w,
            height=h,
            is_native_hdr=False,
            source_bit_depth=8,
            format_name=format_label,
            filename=filename,
            file_size_bytes=file_size,
            exif=exif_data,
            dynamic_range_stops=8.0
        )
