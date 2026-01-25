"""
Entry point for running Semantic OS shell.

Usage:
    python -m kernel
    python -m kernel --db semantic.db
    python -m kernel --provider lm_studio
"""

from .shell import main

if __name__ == "__main__":
    main()
