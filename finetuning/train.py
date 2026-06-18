#!/usr/bin/env python3
"""
MarlOS Personalized Autocomplete Trainer

Fine-tunes Qwen2-0.5B-Instruct on personal chat data for autocomplete.
Uses LoRA for efficient training on consumer GPUs.

Usage:
    python train.py --data_path ./data/export.jsonl --output_dir ./output
"""

import argparse
import json
import os
import sys
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

import torch
from transformers import (
    AutoTokenizer,
    AutoModelForCausalLM,
    TrainingArguments,
    Trainer,
    DataCollatorForLanguageModeling,
)
from peft import (
    LoraConfig,
    get_peft_model,
    TaskType,
    prepare_model_for_kbit_training,
)
from datasets import Dataset
from tqdm import tqdm


@dataclass
class TrainingConfig:
    """Training configuration optimized for autocomplete."""

    # Model settings
    model_name: str = "Qwen/Qwen2-0.5B-Instruct"
    max_seq_length: int = 512

    # LoRA settings (memory-efficient fine-tuning)
    lora_r: int = 16  # Rank
    lora_alpha: int = 32
    lora_dropout: float = 0.05
    lora_target_modules: tuple = (
        "q_proj",
        "k_proj",
        "v_proj",
        "o_proj",
        "gate_proj",
        "up_proj",
        "down_proj",
    )

    # Training hyperparameters
    num_train_epochs: int = 3
    per_device_train_batch_size: int = 8
    per_device_eval_batch_size: int = 8
    gradient_accumulation_steps: int = 2
    learning_rate: float = 5e-5
    warmup_steps: int = 100
    weight_decay: float = 0.01
    max_grad_norm: float = 1.0

    # Hardware settings
    fp16: bool = True  # Use mixed precision on GPU
    bf16: bool = False
    gradient_checkpointing: bool = True

    # Output settings
    output_dir: str = "./output"
    logging_steps: int = 10
    save_steps: int = 100
    eval_steps: int = 100


def load_conversations(data_path: str) -> list[dict]:
    """Load conversations from JSONL export file."""
    conversations = []

    with open(data_path, "r", encoding="utf-8") as f:
        for line in tqdm(f, desc="Loading conversations"):
            try:
                conv = json.loads(line.strip())
                if conv.get("messages") and len(conv["messages"]) >= 2:
                    conversations.append(conv)
            except json.JSONDecodeError:
                continue

    print(f"Loaded {len(conversations)} conversations")
    return conversations


def prepare_training_data(
    conversations: list[dict],
    tokenizer: AutoTokenizer,
    max_length: int = 512,
) -> Dataset:
    """
    Convert conversations to training format for Qwen2 chat template.

    Qwen2 uses: <|im_start|>role\ncontent<|im_end|>
    """

    def format_conversation(conv: dict) -> str:
        messages = conv.get("messages", [])
        formatted = ""

        for msg in messages:
            role = msg.get("role", "user")
            content = msg.get("content", "")

            # Skip empty messages
            if not content.strip():
                continue

            formatted += f"<|im_start|>{role}\n{content}<|im_end|>\n"

        return formatted

    # Format all conversations
    texts = []
    for conv in tqdm(conversations, desc="Formatting conversations"):
        text = format_conversation(conv)
        texts.append(text)

    # Tokenize
    tokenized = tokenizer(
        texts,
        max_length=max_length,
        truncation=True,
        padding="max_length",
        return_tensors="pt",
    )

    # Create dataset
    dataset = Dataset.from_dict({
        "input_ids": tokenized["input_ids"],
        "attention_mask": tokenized["attention_mask"],
        "labels": tokenized["input_ids"].clone(),  # For causal LM, labels = input_ids
    })

    # Split into train/validation
    split = dataset.train_test_split(test_size=0.1, seed=42)
    return split["train"], split["test"]


def setup_model_and_tokenizer(config: TrainingConfig):
    """Load model and tokenizer, apply LoRA configuration."""

    print(f"Loading model: {config.model_name}")

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(
        config.model_name,
        trust_remote_code=True,
    )

    # Set pad token if not present
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token
        tokenizer.pad_token_id = tokenizer.eos_token_id

    # Load model
    model = AutoModelForCausalLM.from_pretrained(
        config.model_name,
        torch_dtype=torch.float16 if config.fp16 else torch.float32,
        device_map="auto",
        trust_remote_code=True,
    )

    print(f"Model loaded on {model.device}")

    # Prepare for k-bit training (optional, for larger models)
    model = prepare_model_for_kbit_training(model)

    # Configure LoRA
    lora_config = LoraConfig(
        r=config.lora_r,
        lora_alpha=config.lora_alpha,
        lora_dropout=config.lora_dropout,
        target_modules=config.lora_target_modules,
        task_type=TaskType.CAUSAL_LM,
        inference_mode=False,
    )

    # Apply LoRA to model
    model = get_peft_model(model, lora_config)
    model.print_trainable_parameters()

    return model, tokenizer


def train_model(
    model,
    tokenizer,
    train_dataset: Dataset,
    eval_dataset: Dataset,
    config: TrainingConfig,
):
    """Train the model with the given configuration."""

    training_args = TrainingArguments(
        output_dir=config.output_dir,
        num_train_epochs=config.num_train_epochs,
        per_device_train_batch_size=config.per_device_train_batch_size,
        per_device_eval_batch_size=config.per_device_eval_batch_size,
        gradient_accumulation_steps=config.gradient_accumulation_steps,
        learning_rate=config.learning_rate,
        warmup_steps=config.warmup_steps,
        weight_decay=config.weight_decay,
        max_grad_norm=config.max_grad_norm,
        fp16=config.fp16,
        bf16=config.bf16,
        gradient_checkpointing=config.gradient_checkpointing,
        logging_steps=config.logging_steps,
        save_steps=config.save_steps,
        eval_steps=config.eval_steps,
        save_total_limit=2,
        load_best_model_at_end=True,
        metric_for_best_model="eval_loss",
        greater_is_better=False,
        report_to="none",  # Disable wandb/tensorboard
        save_safetensors=True,
    )

    data_collator = DataCollatorForLanguageModeling(
        tokenizer=tokenizer,
        mlm=False,  # Causal LM, not masked LM
    )

    trainer = Trainer(
        model=model,
        args=training_args,
        train_dataset=train_dataset,
        eval_dataset=eval_dataset,
        data_collator=data_collator,
        tokenizer=tokenizer,
    )

    print("Starting training...")
    train_result = trainer.train()

    print("Training completed!")
    print(f"Final training loss: {train_result.training_loss:.4f}")

    # Save final model
    trainer.save_model(os.path.join(config.output_dir, "final_model"))
    tokenizer.save_pretrained(os.path.join(config.output_dir, "final_model"))

    # Save training metrics
    metrics = train_result.metrics
    with open(os.path.join(config.output_dir, "training_metrics.json"), "w") as f:
        json.dump(metrics, f, indent=2)

    return trainer


def main():
    parser = argparse.ArgumentParser(
        description="Fine-tune Qwen2 for personalized autocomplete"
    )
    parser.add_argument(
        "--data_path",
        type=str,
        required=True,
        help="Path to JSONL file with exported conversations",
    )
    parser.add_argument(
        "--output_dir",
        type=str,
        default="./output",
        help="Output directory for the fine-tuned model",
    )
    parser.add_argument(
        "--epochs",
        type=int,
        default=3,
        help="Number of training epochs",
    )
    parser.add_argument(
        "--batch_size",
        type=int,
        default=8,
        help="Per-device batch size",
    )
    parser.add_argument(
        "--learning_rate",
        type=float,
        default=5e-5,
        help="Learning rate",
    )
    parser.add_argument(
        "--max_length",
        type=int,
        default=512,
        help="Maximum sequence length",
    )

    args = parser.parse_args()

    # Create output directory
    os.makedirs(args.output_dir, exist_ok=True)

    # Load configuration
    config = TrainingConfig(
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch_size,
        learning_rate=args.learning_rate,
        max_seq_length=args.max_length,
        output_dir=args.output_dir,
    )

    print(f"Configuration: {config}")

    # Load data
    conversations = load_conversations(args.data_path)
    if len(conversations) == 0:
        print("No valid conversations found!")
        sys.exit(1)

    # Setup model and tokenizer
    model, tokenizer = setup_model_and_tokenizer(config)

    # Prepare datasets
    train_dataset, eval_dataset = prepare_training_data(
        conversations, tokenizer, config.max_seq_length
    )

    print(f"Training samples: {len(train_dataset)}")
    print(f"Validation samples: {len(eval_dataset)}")

    # Train
    train_model(model, tokenizer, train_dataset, eval_dataset, config)

    print(f"\nTraining complete! Model saved to: {config.output_dir}")
    print("Next step: Run convert_to_onnx.py to convert for browser inference.")


if __name__ == "__main__":
    main()
