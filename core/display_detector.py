"""core/display_detector.py — Détection matérielle des capacités HDR de l'écran sous Windows.

Utilise l'API Windows CCD (Connecting and Configuring Displays) via ctypes pour interroger:
- Si l'écran prend en charge le HDR matériel (advancedColorSupported).
- Si le mode HDR est actuellement activé dans Windows (advancedColorEnabled).
- La profondeur de couleur en bits par composante (BitsPerColor).
"""
from __future__ import annotations

import ctypes
from ctypes import wintypes, Structure, c_uint32, sizeof, byref
from typing import Dict, Any, List


class LUID(Structure):
    _fields_ = [('LowPart', wintypes.DWORD), ('HighPart', wintypes.LONG)]


class DISPLAYCONFIG_RATIONAL(Structure):
    _fields_ = [('Numerator', c_uint32), ('Denominator', c_uint32)]


class DISPLAYCONFIG_PATH_SOURCE_INFO(Structure):
    _fields_ = [
        ('adapterId', LUID),
        ('id', c_uint32),
        ('modeInfoIdx', c_uint32),
        ('statusFlags', c_uint32)
    ]


class DISPLAYCONFIG_PATH_TARGET_INFO(Structure):
    _fields_ = [
        ('adapterId', LUID),
        ('id', c_uint32),
        ('modeInfoIdx', c_uint32),
        ('outputTechnology', c_uint32),
        ('rotation', c_uint32),
        ('scaling', c_uint32),
        ('refreshRate', DISPLAYCONFIG_RATIONAL),
        ('scanLineOrdering', c_uint32),
        ('targetAvailable', wintypes.BOOL),
        ('statusFlags', c_uint32),
    ]


class DISPLAYCONFIG_PATH_INFO(Structure):
    _fields_ = [
        ('sourceInfo', DISPLAYCONFIG_PATH_SOURCE_INFO),
        ('targetInfo', DISPLAYCONFIG_PATH_TARGET_INFO),
        ('flags', c_uint32),
    ]


class DISPLAYCONFIG_DEVICE_INFO_HEADER(Structure):
    _fields_ = [
        ('type', c_uint32),
        ('size', c_uint32),
        ('adapterId', LUID),
        ('id', c_uint32)
    ]


class DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO(Structure):
    _fields_ = [
        ('header', DISPLAYCONFIG_DEVICE_INFO_HEADER),
        ('value', c_uint32),  # Bit 0: supported, Bit 1: enabled, Bit 2: wideColorEnforced
        ('colorEncoding', c_uint32),
        ('bitsPerColor', c_uint32),
    ]


def get_display_hdr_status() -> Dict[str, Any]:
    """Interroge la configuration Windows pour détecter le support et l'état HDR de tous les écrans actifs.

    Retourne:
        dict contenant:
          - has_hdr_enabled: bool (au moins un écran avec HDR actif)
          - has_hdr_supported: bool (au moins un écran compatible HDR)
          - displays: list des caractéristiques de chaque écran
          - recommended_mode: 'true_hdr' | 'simulation_sdr'
          - message: explication textuelle destinée à l'utilisateur
    """
    displays: List[Dict[str, Any]] = []
    has_hdr_supported = False
    has_hdr_enabled = False

    try:
        user32 = ctypes.windll.user32
        numPaths = c_uint32()
        numModes = c_uint32()
        QDC_ONLY_ACTIVE_PATHS = 2

        res_buffer = user32.GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, byref(numPaths), byref(numModes))
        if res_buffer == 0 and numPaths.value > 0:
            paths = (DISPLAYCONFIG_PATH_INFO * numPaths.value)()
            modeBuffer = (ctypes.c_byte * (numModes.value * 72))()

            query_res = user32.QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS, byref(numPaths), paths, byref(numModes), modeBuffer, None
            )

            if query_res == 0:
                for i in range(numPaths.value):
                    path = paths[i]
                    color_info = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO()
                    color_info.header.type = 9  # DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO
                    color_info.header.size = sizeof(DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO)
                    color_info.header.adapterId = path.targetInfo.adapterId
                    color_info.header.id = path.targetInfo.id

                    hr = user32.DisplayConfigGetDeviceInfo(byref(color_info.header))
                    if hr == 0:
                        val = color_info.value
                        supp = bool(val & 1)
                        enab = bool(val & 2)
                        bits = int(color_info.bitsPerColor)
                        if supp:
                            has_hdr_supported = True
                        if enab:
                            has_hdr_enabled = True

                        displays.append({
                            "index": i,
                            "hdr_supported": supp,
                            "hdr_enabled": enab,
                            "bits_per_color": bits,
                            "color_encoding": int(color_info.colorEncoding)
                        })
    except Exception as e:
        # En cas d'erreur de requête système
        return {
            "has_hdr_enabled": False,
            "has_hdr_supported": False,
            "displays": [],
            "recommended_mode": "simulation_sdr",
            "message": f"Impossible d'interroger la configuration Windows ({e}). Mode simulation activé.",
            "shortcut_tip": None
        }

    if has_hdr_enabled:
        rec_mode = "true_hdr"
        msg = "Écran HDR actif et prêt (Windows HDR activé)."
        tip = None
    elif has_hdr_supported:
        rec_mode = "simulation_sdr"
        msg = "Votre écran est compatible HDR matériel, mais le HDR Windows est désactivé."
        tip = "Astuce : Appuyez sur Win + Alt + B pour activer le HDR dans Windows."
    else:
        rec_mode = "simulation_sdr"
        msg = "Écran SDR standard détecté. Mode simulation de la dynamique maximale activé."
        tip = None

    return {
        "has_hdr_enabled": has_hdr_enabled,
        "has_hdr_supported": has_hdr_supported,
        "displays": displays,
        "recommended_mode": rec_mode,
        "message": msg,
        "shortcut_tip": tip
    }


if __name__ == "__main__":
    import json
    info = get_display_hdr_status()
    print(json.dumps(info, indent=2))
