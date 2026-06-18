#!/usr/bin/env python3
"""
Convert Fine-Tuned Model to ONNX

Converts a fine-tuned Qwen2 model with LoRA adapters to ONNX format
for browser inference via transformers.js.

Usage:
    python convert_to_onnx.py --model_path ./output/final_model --output_dir ./onnx_model
"""

import argparse
import os
import shutil
from pathlib import Path

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer
from peft import PeftModel


def merge_lora_and_convert_to_onnx(
    base_model_name: str,
    lora_model_path: str,
    output_dir: str,
):
    """
    Merge LoRA adapters with base model and convert to ONNX.

    Args:
        base_model_name: HuggingFace model ID (e.g., "Qwen/Qwen2-0.5B-Instruct")
        lora_model_path: Path to the fine-tuned LoRA model
        output_dir: Where to save the ONNX model
    """

    print(f"Loading base model: {base_model_name}")
    print(f"Loading LoRA adapters from: {lora_model_path}")

    # Create output directory
    os.makedirs(output_dir, exist_ok=True)

    # Load base model
    base_model = AutoModelForCausalLM.from_pretrained(
        base_model_name,
        torch_dtype=torch.float32,  # Use float32 for ONNX export
        device_map="auto",
        trust_remote_code=True,
    )

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(
        base_model_name,
        trust_remote_code=True,
    )

    # Load and merge LoRA adapters
    print("Merging LoRA adapters...")
    model = PeftModel.from_pretrained(base_model, lora_model_path)
    model = model.merge_and_unload()  # Merge adapters into base model

    print("Converting to ONNX...")

    # Use Optimum for ONNX conversion
    try:
        from optimum.onnxruntime import ORTModelForCausalLM

        # Convert model
        model = ORTModelForCausalLM.from_pretrained(
            model_id=model.config._name_or_path,
            export=True,
            use_merged=True,
            output_dir=output_dir,
        )

        # Save tokenizer
        tokenizer.save_pretrained(output_dir)

        print(f"Model converted and saved to: {output_dir}")
        return True

    except ImportError:
        print("Optimum not available, trying torch.onnx export...")
        return convert_with_torch_onnx(model, tokenizer, output_dir)


def convert_with_torch_onnx(model, tokenizer, output_dir: str) -> bool:
    """Fallback ONNX conversion using torch.onnx.export."""

    import torch.onnx

    # Prepare model for export
    model.eval()
    model.config.return_dict = True

    # Create dummy inputs
    batch_size = 1
    seq_length = 128
    dummy_input_ids = torch.randint(
        0, model.config.vocab_size, (batch_size, seq_length)
    )
    dummy_attention_mask = torch.ones_like(dummy_input_ids)

    # Export to ONNX
    onnx_path = os.path.join(output_dir, "model.onnx")

    torch.onnx.export(
        model,
        (dummy_input_ids, dummy_attention_mask),
        onnx_path,
        export_params=True,
        opset_version=17,
        input_names=["input_ids", "attention_mask"],
        output_names=["logits"],
        dynamic_axes={
            "input_ids": {0: "batch_size", 1: "sequence_length"},
            "attention_mask": {0: "batch_size", 1: "sequence_length"},
            "logits": {0: "batch_size", 1: "sequence_length"},
        },
    )

    # Save tokenizer and config
    tokenizer.save_pretrained(output_dir)
    model.config.save_pretrained(output_dir)

    print(f"ONNX model saved to: {onnx_path}")
    return True


def optimize_for_browser(output_dir: str):
    """
    Optimize ONNX model for browser inference.

    This includes quantization to reduce model size.
    """

    try:
        import onnxruntime as ort

        onnx_path = os.path.join(output_dir, "model.onnx")

        # Create quantization options
        from onnxruntime.quantization import quantize_dynamic, QuantType

        quantized_path = os.path.join(output_dir, "model_quantized.onnx")

        print("Quantizing model for browser...")
        quantize_dynamic(
            onnx_path,
            quantized_path,
            weight_type=QuantType.QUInt8,  # 8-bit quantization
            optimize_model=True,
        )

        # Replace original with quantized
        shutil.move(quantized_path, onnx_path)
        print(f"Quantized model saved to: {onnx_path}")

    except ImportError:
        print("ONNX Runtime not available, skipping quantization")
    except Exception as e:
        print(f"Quantization failed: {e}")


def generate_model_files(output_dir: str):
    """Generate additional files needed for transformers.js."""

    # Create model configuration for transformers.js
    config = {
        "model_type": "qwen2",
        "architectures": ["Qwen2ForCausalLM"],
        "hidden_size": 896,
        "num_attention_heads": 14,
        "num_hidden_layers": 24,
        "vocab_size": 151936,
        "max_position_embeddings": 32768,
        "use_cache": True,
    }

    import json
    with open(os.path.join(output_dir, "config.json"), "w") as f:
        json.dump(config, f, indent=2)

    print("Generated model configuration files")


def main():
    parser = argparse.ArgumentParser(
        description="Convert fine-tuned model to ONNX format"
    )
    parser.add_argument(
        "--model_path",
        type=str,
        required=True,
        help="Path to fine-tuned model directory",
    )
    parser.add_argument(
        "--output_dir",
        type=str,
        default="./onnx_model",
        help="Output directory for ONNX model",
    )
    parser.add_argument(
        "--base_model",
        type=str,
        default="Qwen/Qwen2-0.5B-Instruct",
        help="Base model name (for loading weights)",
    )
    parser.add_argument(
        "--quantize",
        action="store_true",
        help="Apply quantization for smaller model size",
    )

    args = parser.parse_args()

    # Convert to ONNX
    success = merge_lora_and_convert_to_onnx(
        args.base_model,
        args.model_path,
        args.output_dir,
    )

    if not success:
        print("ONNX conversion failed!")
        return 1

    # Optionally quantize
    if args.quantize:
        optimize_for_browser(args.output_dir)

    # Generate additional files
    generate_model_files(args.output_dir)

    print("\n" + "="*50)
    print("ONNX conversion complete!")
    print(f"Model saved to: {args.output_dir}")
    print("\nFor browser inference, copy this directory to:")
    print("  MarlOS/src-tauri/assets/models/")
    print("="*50)

    return 0


if __name__ == "__main__":
    exit(main())
