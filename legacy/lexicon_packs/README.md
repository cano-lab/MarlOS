WordNet Lexicon Pack

This folder is for local lexicon packs loaded by the editor.

Recommended pack: WordNet (Princeton)
- License: WordNet License (commercial use permitted with attribution)
- Attribution: "WordNet 3.0 from Princeton University"

Build a pack locally:
1) Install nltk: `pip install nltk`
2) Download WordNet data:
   Python:
     >>> import nltk
     >>> nltk.download("wordnet")
3) Run the builder:
   `python scripts/build_wordnet_pack.py`

This will create:
- lexicon_packs/wordnet_lexicon.json

Note: Do not commit WordNet data files themselves; only the generated JSON pack.
