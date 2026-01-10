#!/usr/bin/env python3
import json
from collections import defaultdict
from pathlib import Path


def main():
    try:
        from nltk.corpus import wordnet as wn
    except Exception:
        print("nltk and wordnet are required. Install nltk and download wordnet.")
        raise SystemExit(1)

    entries = defaultdict(lambda: {"definition": "", "synonyms": []})

    for synset in wn.all_synsets():
        definition = synset.definition().strip()
        for lemma in synset.lemmas():
            key = lemma.name().replace("_", " ").lower()
            entry = entries[key]
            if not entry["definition"]:
                entry["definition"] = definition
            synonyms = {l.name().replace("_", " ") for l in synset.lemmas()}
            entry["synonyms"] = sorted(set(entry["synonyms"]).union(synonyms))

    output_path = Path(__file__).resolve().parent.parent / "lexicon_packs" / "wordnet_lexicon.json"
    output_path.write_text(json.dumps(entries, indent=2), encoding="utf-8")
    print(f"Wrote {output_path}")


if __name__ == "__main__":
    main()
