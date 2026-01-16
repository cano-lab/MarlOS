"""
MarlOS License Manager
======================

Manages feature tiers and license validation.

Tiers:
- Free: Basic editor, preview, local AI
- Pro: Full semantic memory, screen memory, analytics
- Team: Collaboration features (future)
"""

import json
import platform
import hashlib
from pathlib import Path
from enum import Enum
from typing import Optional, Dict, List, Set
from dataclasses import dataclass


class LicenseTier(str, Enum):
    """MarlOS license tiers."""
    FREE = "free"
    PRO = "pro"
    TEAM = "team"


@dataclass
class License:
    """MarlOS license."""
    tier: LicenseTier
    user_email: str
    license_key: str
    installation_id: str
    expires_at: Optional[str] = None  # ISO format, None = perpetual
    features: Set[str] = None

    def is_valid(self) -> bool:
        """Check if license is valid."""
        # For now, all licenses are valid
        # In production, check expiration, signature, etc.
        return True

    def has_feature(self, feature: str) -> bool:
        """Check if license has access to a feature."""
        # Team features
        if feature.startswith("team."):
            return self.tier == LicenseTier.TEAM

        # Pro features
        if feature.startswith("pro."):
            return self.tier in (LicenseTier.PRO, LicenseTier.TEAM)

        # Free features (available to all)
        return True


class LicenseManager:
    """Manages MarlOS licenses and feature access."""

    # Feature definitions
    FEATURES = {
        # Free features
        "free.editor": "Markdown editor with live preview",
        "free.preview": "HTML preview",
        "free.browse": "File browser",
        "free.local_ai": "Local AI via LM Studio",
        "free.workspace": "Workspace management",

        # Pro features
        "pro.semantic_memory": "Full semantic memory with embeddings",
        "pro.screen_memory": "Visual memory capture (screenshots)",
        "pro.workflow_analytics": "Workflow pattern analysis",
        "pro.focus_tracking": "Document focus tracking",
        "pro.relation_graph": "File relationship graph",
        "pro.context_export": "Export context for AI assistants",
        "pro.advanced_search": "Semantic search across all files",
        "pro.priority_support": "Priority email support",

        # Team features (future)
        "team.shared_workspace": "Shared team workspaces",
        "team.collaboration": "Real-time collaboration",
        "team.shared_memory": "Team semantic memory",
        "team.analytics": "Team analytics dashboard",
        "team.admin": "Admin controls and permissions",
    }

    def __init__(self):
        self._license: Optional[License] = None
        self._license_file: Path = self._get_license_file()
        self._installation_id = self._get_installation_id()

        # Load license on startup
        self._load_license()

    def _get_license_file(self) -> Path:
        """Get license file path."""
        if platform.system() == "Windows":
            base = Path.home() / "AppData" / "Local" / "MarlOS"
        else:
            base = Path.home() / ".marlos"

        base.mkdir(parents=True, exist_ok=True)
        return base / "license.json"

    def _get_installation_id(self) -> str:
        """Generate unique installation ID."""
        # Based on machine-specific info
        data = f"{platform.node()}-{platform.machine()}-{platform.system()}"
        return hashlib.sha256(data.encode()).hexdigest()[:16]

    def _load_license(self):
        """Load license from file."""
        if not self._license_file.exists():
            # No license = Free tier
            self._license = License(
                tier=LicenseTier.FREE,
                user_email="",
                license_key="free",
                installation_id=self._installation_id,
                features=set()
            )
            return

        try:
            with open(self._license_file, 'r') as f:
                data = json.load(f)

            # Validate installation ID matches
            if data.get("installation_id") != self._installation_id:
                print(f"[License] Warning: Installation ID mismatch")
                self._license = License(
                    tier=LicenseTier.FREE,
                    user_email="",
                    license_key="free",
                    installation_id=self._installation_id,
                    features=set()
                )
                return

            self._license = License(
                tier=LicenseTier(data.get("tier", "free")),
                user_email=data.get("user_email", ""),
                license_key=data.get("license_key", ""),
                installation_id=data.get("installation_id", ""),
                expires_at=data.get("expires_at"),
                features=set(data.get("features", []))
            )

        except Exception as e:
            print(f"[License] Error loading license: {e}")
            self._license = License(
                tier=LicenseTier.FREE,
                user_email="",
                license_key="free",
                installation_id=self._installation_id,
                features=set()
            )

    def save_license(self, tier: LicenseTier, user_email: str, license_key: str):
        """Save license to file."""
        self._license = License(
            tier=tier,
            user_email=user_email,
            license_key=license_key,
            installation_id=self._installation_id,
            expires_at=None  # Perpetual for now
        )

        try:
            data = {
                "tier": self._license.tier.value,
                "user_email": self._license.user_email,
                "license_key": self._license.license_key,
                "installation_id": self._license.installation_id,
                "expires_at": self._license.expires_at,
                "features": list(self._license.features) if self._license.features else []
            }

            with open(self._license_file, 'w') as f:
                json.dump(data, f, indent=2)

        except Exception as e:
            print(f"[License] Error saving license: {e}")

    def get_tier(self) -> LicenseTier:
        """Get current license tier."""
        return self._license.tier if self._license else LicenseTier.FREE

    def has_feature(self, feature: str) -> bool:
        """Check if current license has access to a feature."""
        if not self._license or not self._license.is_valid():
            return False
        return self._license.has_feature(feature)

    def require_feature(self, feature: str) -> bool:
        """Check feature access and show upgrade prompt if needed."""
        if self.has_feature(feature):
            return True

        # Feature not available - return False to show upgrade dialog
        return False

    def get_feature_description(self, feature: str) -> str:
        """Get description of a feature."""
        return self.FEATURES.get(feature, "Unknown feature")

    def get_available_features(self) -> Dict[str, str]:
        """Get all features available to current tier."""
        if not self._license:
            return {}

        available = {}
        for feat_key, feat_desc in self.FEATURES.items():
            if self._license.has_feature(feat_key):
                available[feat_key] = feat_desc

        return available

    def get_locked_features(self) -> Dict[str, str]:
        """Get features not available in current tier."""
        if not self._license:
            return {}

        locked = {}
        for feat_key, feat_desc in self.FEATURES.items():
            if not self._license.has_feature(feat_key):
                locked[feat_key] = feat_desc

        return locked

    def is_free_tier(self) -> bool:
        """Check if user is on Free tier."""
        return self.get_tier() == LicenseTier.FREE

    def is_pro_tier(self) -> bool:
        """Check if user is on Pro tier."""
        return self.get_tier() in (LicenseTier.PRO, LicenseTier.TEAM)

    def is_team_tier(self) -> bool:
        """Check if user is on Team tier."""
        return self.get_tier() == LicenseTier.TEAM

    def get_upgrade_message(self, feature: str) -> str:
        """Get message for feature upgrade."""
        tier = self.get_tier()

        if tier == LicenseTier.FREE:
            return (
                f"This feature requires MarlOS Pro.\n\n"
                f"Feature: {self.get_feature_description(feature)}\n\n"
                f"Upgrade to unlock:\n"
                f"• Full semantic memory\n"
                f"• Screen memory capture\n"
                f"• Workflow analytics\n"
                f"• Priority support\n\n"
                f"One-time purchase: $49"
            )
        elif tier == LicenseTier.PRO and feature.startswith("team."):
            return (
                f"This feature requires MarlOS Team.\n\n"
                f"Feature: {self.get_feature_description(feature)}\n\n"
                f"Upgrade to unlock:\n"
                f"• Shared workspaces\n"
                f"• Real-time collaboration\n"
                f"• Team semantic memory\n"
                f"• Admin controls\n\n"
                f"$99/user/year"
            )

        return "This feature is not available in your tier."

    def activate_license(self, license_key: str, email: str) -> tuple[bool, str]:
        """Activate a license key.

        Returns:
            (success, message)
        """
        # For now, simple validation
        # In production, this would verify against a server

        # Demo license keys for testing
        # (In production, these would be cryptographically signed)
        if license_key.startswith("MARLOS-PRO-"):
            # Pro license
            self.save_license(LicenseTier.PRO, email, license_key)
            return True, "MarlOS Pro activated successfully!"

        elif license_key.startswith("MARLOS-TEAM-"):
            # Team license
            self.save_license(LicenseTier.TEAM, email, license_key)
            return True, "MarlOS Team activated successfully!"

        else:
            return False, "Invalid license key. Please check and try again."

    def deactivate_license(self):
        """Remove current license (revert to Free)."""
        try:
            if self._license_file.exists():
                self._license_file.unlink()
            self._license = License(
                tier=LicenseTier.FREE,
                user_email="",
                license_key="free",
                installation_id=self._installation_id,
                features=set()
            )
            return True, "License deactivated. reverted to Free tier."
        except Exception as e:
            return False, f"Error deactivating license: {e}"


# Global license manager instance
_license_manager: Optional[LicenseManager] = None


def get_license_manager() -> LicenseManager:
    """Get the global license manager instance."""
    global _license_manager
    if _license_manager is None:
        _license_manager = LicenseManager()
    return _license_manager
