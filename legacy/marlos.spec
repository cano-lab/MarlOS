# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec file for MarlOS
"""
import sys
import os

block_cipher = None

a = Analysis(
    ['viewer.py'],
    pathex=[],
    binaries=[],
    datas=[
        ('ARCHITECTURE.md', '.'),
        ('USER_GUIDE.md', '.'),
        ('SURVEY.md', '.'),
        ('ide', 'ide'),
        ('kernel', 'kernel'),
    ],
    hiddenimports=[
        'PyQt6.QtCore',
        'PyQt6.QtGui',
        'PyQt6.QtWidgets',
        'PyQt6.QtWebEngineCore',
        'PyQt6.QtWebEngineWidgets',
        'PyQt6.QtPrintSupport',
        'chromium',
        'markdown',
        'PIL',
        'PIL._imaging',
        'sentencepiece',
        'torch',
        'transformers',
        'chromadb',
        'sentence_transformers',
        'tiktoken',
        'openai',
        'numpy',
        'pandas',
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        'Tkinter',
        'matplotlib',
        'scipy',
        'pytest',
        'setuptools',
    ],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='MarlOS',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,  # Set to True to see console output for debugging
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon='icon.ico' if os.path.exists('icon.ico') else None,
    version='version_info.txt' if os.path.exists('version_info.txt') else None,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='MarlOS',
    version='version_info.txt' if os.path.exists('version_info.txt') else None,
)
