#!/usr/bin/env python3
"""
Dataset Preparation for Fine-Tuning

Prepares and validates chat data for training.
Includes data cleaning, deduplication, and train/validation splits.

Usage:
    python prepare_dataset.py --input ./data/raw_export.jsonl --output ./data/prepared
"""

import argparse
import json
import os
import random
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import List, Dict

from tqdm import tqdm


@dataclass
class DatasetStats:
    """Statistics about the prepared dataset."""

    total_conversations: int
    total_messages: int
    avg_messages_per_conv: float
    user_messages: int
    assistant_messages: int
    total_tokens: int
    avg_tokens_per_message: float
    providers: Dict[str, int]


def load_jsonl(file_path: str) -> List[dict]:
    """Load JSONL file."""
    data = []
    with open(file_path, "r", encoding="utf-8") as f:
        for line in f:
            try:
                data.append(json.loads(line.strip()))
            except json.JSONDecodeError:
                continue
    return data


def save_jsonl(data: List[dict], file_path: str):
    """Save data to JSONL file."""
    os.makedirs(os.path.dirname(file_path), exist_ok=True)
    with open(file_path, "w", encoding="utf-8") as f:
        for item in data:
            f.write(json.dumps(item, ensure_ascii=False) + "\n")


def clean_text(text: str) -> str:
    """Clean and normalize text content."""

    # Remove excessive whitespace
    text = " ".join(text.split())

    # Remove control characters except newlines
    text = "".join(c for c in text if c.isprintable() or c in "\n\r\t")

    return text.strip()


def validate_conversation(conv: dict) -> bool:
    """Check if a conversation is valid for training."""

    messages = conv.get("messages", [])

    # Must have at least 2 messages (user + assistant)
    if len(messages) < 2:
        return False

    # Must have both user and assistant messages
    roles = set(msg.get("role", "") for msg in messages)
    if "user" not in roles or "assistant" not in roles:
        return False

    # Check message content
    for msg in messages:
        content = msg.get("content", "")
        if not content or len(content.strip()) < 2:
            return False

    return True


def deduplicate_conversations(
    conversations: List[dict],
    min_similarity: float = 0.9,
) -> List[dict]:
    """
    Remove near-duplicate conversations based on content similarity.

    For now, uses simple exact matching on first message.
    Can be enhanced with fuzzy matching.
    """

    seen = set()
    unique = []

    for conv in conversations:
        messages = conv.get("messages", [])
        if not messages:
            continue

        # Use first user message as signature
        first_user_msg = next(
            (m["content"] for m in messages if m.get("role") == "user"),
            None,
        )

        if first_user_msg and first_user_msg not in seen:
            seen.add(first_user_msg)
            unique.append(conv)

    return unique


def balance_conversations(
    conversations: List[dict],
    max_per_provider: int = 1000,
    min_per_provider: int = 10,
) -> List[dict]:
    """Balance dataset across different providers."""

    # Group by provider
    by_provider: Dict[str, List[dict]] = {}
    for conv in conversations:
        provider = conv.get("metadata", {}).get("provider", "unknown")
        by_provider.setdefault(provider, []).append(conv)

    # Limit per provider
    balanced = []
    for provider, convs in by_provider.items():
        # Random sample if too many
        if len(convs) > max_per_provider:
            random.shuffle(convs)
            convs = convs[:max_per_provider]

        # Skip if too few
        if len(convs) >= min_per_provider:
            balanced.extend(convs)

    return balanced


def calculate_statistics(conversations: List[dict]) -> DatasetStats:
    """Calculate dataset statistics."""

    total_messages = 0
    user_messages = 0
    assistant_messages = 0
    total_tokens = 0
    providers: Dict[str, int] = {}

    for conv in conversations:
        messages = conv.get("messages", [])
        total_messages += len(messages)

        for msg in messages:
            role = msg.get("role", "")
            content = msg.get("content", "")

            if role == "user":
                user_messages += 1
            elif role == "assistant":
                assistant_messages += 1

            # Approximate token count (1 token ≈ 4 characters)
            total_tokens += len(content) // 4

        # Count providers
        provider = conv.get("metadata", {}).get("provider", "unknown")
        providers[provider] = providers.get(provider, 0) + 1

    return DatasetStats(
        total_conversations=len(conversations),
        total_messages=total_messages,
        avg_messages_per_conv=total_messages / len(conversations) if conversations else 0,
        user_messages=user_messages,
        assistant_messages=assistant_messages,
        total_tokens=total_tokens,
        avg_tokens_per_message=total_tokens / total_messages if total_messages else 0,
        providers=providers,
    )


def prepare_dataset(
    input_path: str,
    output_dir: str,
    train_split: float = 0.9,
    seed: int = 42,
) -> Dict:
    """Main dataset preparation pipeline."""

    random.seed(seed)

    # Load raw data
    print(f"Loading data from: {input_path}")
    conversations = load_jsonl(input_path)
    print(f"Loaded {len(conversations)} conversations")

    # Validate
    print("Validating conversations...")
    valid = [c for c in tqdm(conversations) if validate_conversation(c)]
    print(f"Valid conversations: {len(valid)} (removed {len(conversations) - len(valid)})")

    # Clean text
    print("Cleaning text...")
    for conv in tqdm(valid):
        for msg in conv.get("messages", []):
            msg["content"] = clean_text(msg.get("content", ""))

    # Deduplicate
    print("Deduplicating...")
    unique = deduplicate_conversations(valid)
    print(f"Unique conversations: {len(unique)} (removed {len(valid) - len(unique)})")

    # Balance
    print("Balancing by provider...")
    balanced = balance_conversations(unique)
    print(f"Balanced conversations: {len(balanced)}")

    # Calculate statistics
    stats = calculate_statistics(balanced)
    print("\nDataset Statistics:")
    print(f"  Conversations: {stats.total_conversations}")
    print(f"  Total messages: {stats.total_messages}")
    print(f"  Avg messages/conv: {stats.avg_messages_per_conv:.1f}")
    print(f"  User messages: {stats.user_messages}")
    print(f"  Assistant messages: {stats.assistant_messages}")
    print(f"  Approx tokens: {stats.total_tokens:,}")
    print(f"  Providers: {stats.providers}")

    # Split train/validation
    random.shuffle(balanced)
    split_idx = int(len(balanced) * train_split)
    train = balanced[:split_idx]
    val = balanced[split_idx:]

    print(f"\nTrain set: {len(train)} conversations")
    print(f"Validation set: {len(val)} conversations")

    # Save
    os.makedirs(output_dir, exist_ok=True)
    train_path = os.path.join(output_dir, "train.jsonl")
    val_path = os.path.join(output_dir, "val.jsonl")
    stats_path = os.path.join(output_dir, "stats.json")

    save_jsonl(train, train_path)
    save_jsonl(val, val_path)

    with open(stats_path, "w") as f:
        json.dump({
            "total_conversations": stats.total_conversations,
            "total_messages": stats.total_messages,
            "train_conversations": len(train),
            "val_conversations": len(val),
            "providers": stats.providers,
        }, f, indent=2)

    print(f"\nPrepared dataset saved to: {output_dir}")
    print(f"  Train: {train_path}")
    print(f"  Validation: {val_path}")
    print(f"  Stats: {stats_path}")

    return {
        "train_path": train_path,
        "val_path": val_path,
        "stats": stats,
    }


def main():
    parser = argparse.ArgumentParser(
        description="Prepare dataset for fine-tuning"
    )
    parser.add_argument(
        "--input",
        type=str,
        required=True,
        help="Path to raw JSONL export file",
    )
    parser.add_argument(
        "--output",
        type=str,
        default="./data/prepared",
        help="Output directory for prepared dataset",
    )
    parser.add_argument(
        "--train_split",
        type=float,
        default=0.9,
        help="Training data split ratio (0-1)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed",
    )

    args = parser.parse_args()

    prepare_dataset(
        args.input,
        args.output,
        args.train_split,
        args.seed,
    )


if __name__ == "__main__":
    main()
