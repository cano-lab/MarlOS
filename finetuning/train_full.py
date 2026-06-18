#!/usr/bin/env python3
"""
MarlOS Personalized Autocomplete Training Script

This script automates the entire fine-tuning pipeline:
1. Prepares the dataset
2. Trains the model
3. Converts to ONNX
4. Validates the output

Usage:
    python train_full.py --input ../../chat_export.jsonl
"""

import argparse
import os
import sys
import json
import shutil
from pathlib import Path

def check_dependencies():
    """Check if all required packages are installed."""
    required = [
        "torch", "transformers", "peft", "datasets",
        "accelerate", "onnx", "onnxruntime"
    ]

    missing = []
    for package in required:
        try:
            __import__(package)
        except ImportError:
            missing.append(package)

    if missing:
        print(f"❌ Missing packages: {', '.join(missing)}")
        print(f"Run: pip install -r requirements.txt")
        return False

    print("✅ All dependencies installed")
    return True

def run_step(description: str, command: list) -> bool:
    """Run a command step with user confirmation."""
    print(f"\n{'='*60}")
    print(f"Step: {description}")
    print(f"{'='*60}")
    print(f"Command: {' '.join(command)}")

    response = input("\nProceed? [Y/n]: ").strip().lower()
    if response == 'n':
        print("Skipped.")
        return False

    import subprocess
    result = subprocess.run(command, capture_output=True, text=True)

    if result.returncode != 0:
        print(f"❌ Failed!")
        print(result.stderr)
        return False

    print(f"✅ Success!")
    return True

def main():
    parser = argparse.ArgumentParser(description="Train personalized autocomplete model")
    parser.add_argument("--input", required=True, help="Path to exported JSONL file")
    parser.add_argument("--output", default="./model_output", help="Output directory")
    parser.add_argument("--epochs", type=int, default=3, help="Training epochs")
    parser.add_argument("--batch_size", type=int, default=8, help="Batch size")
    parser.add_argument("--skip_prepare", action="store_true", help="Skip data preparation")
    parser.add_argument("--skip_train", action="store_true", help="Skip training")
    parser.add_argument("--skip_convert", action="store_true", help="Skip ONNX conversion")
    parser.add_argument("--auto", action="store_true", help="Run without prompts")

    args = parser.parse_args()

    print("\n" + "="*60)
    print("MarlOS Personalized Autocomplete Training")
    print("="*60)

    # Check dependencies
    if not check_dependencies():
        return 1

    # Create output directories
    data_dir = Path("./data")
    output_dir = Path(args.output)
    onnx_dir = output_dir / "onnx_model"

    success = True

    # Step 1: Prepare dataset
    if not args.skip_prepare:
        if args.auto or run_step("Prepare Dataset", [
            sys.executable, "prepare_dataset.py",
            "--input", args.input,
            "--output", str(data_dir)
        ]):
            if not args.auto:
                result = run_step("Prepare Dataset", [
                    sys.executable, "prepare_dataset.py",
                    "--input", args.input,
                    "--output", str(data_dir)
                ])
                if not result and not args.auto:
                    return 1

    # Step 2: Train model
    if not args.skip_train:
        train_path = data_dir / "train.jsonl"
        if not train_path.exists():
            print(f"❌ Training data not found: {train_path}")
            return 1

        if args.auto or run_step("Train Model", [
            sys.executable, "train.py",
            "--data_path", str(train_path),
            "--output_dir", str(output_dir / "training"),
            "--epochs", str(args.epochs),
            "--batch_size", str(args.batch_size),
        ]):
            if not args.auto:
                result = run_step("Train Model", [
                    sys.executable, "train.py",
                    "--data_path", str(train_path),
                    "--output_dir", str(output_dir / "training"),
                    "--epochs", str(args.epochs),
                    "--batch_size", str(args.batch_size),
                ])
                if not result and not args.auto:
                    return 1

    # Step 3: Convert to ONNX
    if not args.skip_convert:
        model_path = output_dir / "training" / "final_model"
        if not model_path.exists():
            print(f"❌ Trained model not found: {model_path}")
            return 1

        if args.auto or run_step("Convert to ONNX", [
            sys.executable, "convert_to_onnx.py",
            "--model_path", str(model_path),
            "--output_dir", str(onnx_dir),
            "--quantize",
        ]):
            if not args.auto:
                result = run_step("Convert to ONNX", [
                    sys.executable, "convert_to_onnx.py",
                    "--model_path", str(model_path),
                    "--output_dir", str(onnx_dir),
                    "--quantize",
                ])
                if not result and not args.auto:
                    return 1

    # Summary
    print("\n" + "="*60)
    print("Training Complete!")
    print("="*60)
    print(f"\nYour personalized model is ready at:")
    print(f"  {onnx_dir.absolute()}")
    print(f"\nTo use it in MarlOS:")
    print(f"  1. Copy the onnx_model folder to:")
    print(f"     src-tauri/assets/models/")
    print(f"  2. Add the model path in MarlOS settings")
    print("="*60)

    # Save model metadata for MarlOS
    metadata = {
        "id": "personalized-qwen2",
        "name": "Personalized Qwen2",
        "type": "custom",
        "modelPath": str(onnx_dir),
        "createdAt": str(Path.cwd()),
        "quantized": True,
    }

    metadata_file = output_dir / "model_metadata.json"
    with open(metadata_file, "w") as f:
        json.dump(metadata, f, indent=2)

    print(f"\nModel metadata saved to: {metadata_file}")
    print("\nYou can import this metadata in MarlOS to register your model.")

    return 0

if __name__ == "__main__":
    sys.exit(main())
