"""
AI Providers
============

Abstraction layer for AI/LLM providers.
Supports local (LM Studio, Ollama) and cloud (OpenAI, Anthropic).

Usage:
    from kernel.ai_providers import get_provider, LMStudioProvider

    # Auto-detect available provider
    ai = get_provider()

    # Or specific provider
    ai = LMStudioProvider(base_url="http://localhost:1234/v1")

    # Generate response
    response = ai.generate("What files did I edit today?", context=kernel_state)
"""

import os
import json
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Generator
from enum import Enum


class ProviderType(str, Enum):
    """Supported AI providers."""
    LM_STUDIO = "lm_studio"       # Local, OpenAI-compatible
    OLLAMA = "ollama"             # Local
    OPENAI = "openai"             # Cloud
    ANTHROPIC = "anthropic"       # Cloud
    OPENAI_COMPATIBLE = "openai_compatible"  # Generic


@dataclass
class Message:
    """A chat message."""
    role: str  # "system", "user", "assistant"
    content: str


@dataclass
class AIResponse:
    """Response from AI provider."""
    content: str
    model: str = ""
    provider: str = ""
    tokens_used: int = 0
    latency_ms: float = 0
    raw_response: Dict = field(default_factory=dict)


@dataclass
class ProviderConfig:
    """Configuration for an AI provider."""
    provider_type: ProviderType
    base_url: str = ""
    api_key: str = ""
    model: str = ""
    temperature: float = 0.7
    max_tokens: int = 2048
    timeout: int = 60


class AIProvider(ABC):
    """Base class for AI providers."""

    def __init__(self, config: ProviderConfig):
        self.config = config
        self._http_client = None

    @property
    def http_client(self):
        """Lazy-load HTTP client."""
        if self._http_client is None:
            try:
                import httpx
                self._http_client = httpx.Client(timeout=self.config.timeout)
            except ImportError:
                import urllib.request
                self._http_client = "urllib"
        return self._http_client

    @abstractmethod
    def generate(
        self,
        prompt: str,
        context: str = None,
        system_prompt: str = None,
        **kwargs
    ) -> AIResponse:
        """Generate a response."""
        pass

    @abstractmethod
    def chat(
        self,
        messages: List[Message],
        **kwargs
    ) -> AIResponse:
        """Chat with message history."""
        pass

    def stream(
        self,
        prompt: str,
        context: str = None,
        **kwargs
    ) -> Generator[str, None, None]:
        """Stream response (default: non-streaming fallback)."""
        response = self.generate(prompt, context, **kwargs)
        yield response.content

    def is_available(self) -> bool:
        """Check if provider is available."""
        try:
            # Simple health check
            self.generate("test", max_tokens=5)
            return True
        except Exception:
            return False

    def _make_request(self, url: str, data: dict, headers: dict = None) -> dict:
        """Make HTTP request."""
        headers = headers or {}
        headers["Content-Type"] = "application/json"

        if isinstance(self.http_client, str):  # urllib fallback
            import urllib.request
            req = urllib.request.Request(
                url,
                data=json.dumps(data).encode(),
                headers=headers,
                method="POST"
            )
            with urllib.request.urlopen(req, timeout=self.config.timeout) as resp:
                return json.loads(resp.read().decode())
        else:
            resp = self.http_client.post(url, json=data, headers=headers)
            resp.raise_for_status()
            return resp.json()


class LMStudioProvider(AIProvider):
    """LM Studio provider (local, OpenAI-compatible API)."""

    def __init__(
        self,
        base_url: str = "http://localhost:1234/v1",
        model: str = None,  # LM Studio auto-selects loaded model
        **kwargs
    ):
        config = ProviderConfig(
            provider_type=ProviderType.LM_STUDIO,
            base_url=base_url.rstrip("/"),
            model=model or "",
            **kwargs
        )
        super().__init__(config)

    def generate(
        self,
        prompt: str,
        context: str = None,
        system_prompt: str = None,
        **kwargs
    ) -> AIResponse:
        messages = []

        if system_prompt:
            messages.append(Message("system", system_prompt))

        if context:
            messages.append(Message("system", f"Context:\n{context}"))

        messages.append(Message("user", prompt))

        return self.chat(messages, **kwargs)

    def chat(self, messages: List[Message], **kwargs) -> AIResponse:
        start = time.time()

        data = {
            "messages": [{"role": m.role, "content": m.content} for m in messages],
            "temperature": kwargs.get("temperature", self.config.temperature),
            "max_tokens": kwargs.get("max_tokens", self.config.max_tokens),
            "stream": False,
        }

        if self.config.model:
            data["model"] = self.config.model

        url = f"{self.config.base_url}/chat/completions"
        response = self._make_request(url, data)

        latency = (time.time() - start) * 1000

        return AIResponse(
            content=response["choices"][0]["message"]["content"],
            model=response.get("model", "lm-studio"),
            provider="lm_studio",
            tokens_used=response.get("usage", {}).get("total_tokens", 0),
            latency_ms=latency,
            raw_response=response,
        )

    def is_available(self) -> bool:
        """Check if LM Studio is running."""
        try:
            url = f"{self.config.base_url}/models"
            if isinstance(self.http_client, str):
                import urllib.request
                with urllib.request.urlopen(url, timeout=2) as resp:
                    return resp.status == 200
            else:
                resp = self.http_client.get(url, timeout=2)
                return resp.status_code == 200
        except Exception:
            return False


class OllamaProvider(AIProvider):
    """Ollama provider (local)."""

    def __init__(
        self,
        base_url: str = "http://localhost:11434",
        model: str = "llama2",
        **kwargs
    ):
        config = ProviderConfig(
            provider_type=ProviderType.OLLAMA,
            base_url=base_url.rstrip("/"),
            model=model,
            **kwargs
        )
        super().__init__(config)

    def generate(
        self,
        prompt: str,
        context: str = None,
        system_prompt: str = None,
        **kwargs
    ) -> AIResponse:
        start = time.time()

        full_prompt = ""
        if system_prompt:
            full_prompt += f"{system_prompt}\n\n"
        if context:
            full_prompt += f"Context:\n{context}\n\n"
        full_prompt += prompt

        data = {
            "model": self.config.model,
            "prompt": full_prompt,
            "stream": False,
            "options": {
                "temperature": kwargs.get("temperature", self.config.temperature),
                "num_predict": kwargs.get("max_tokens", self.config.max_tokens),
            }
        }

        url = f"{self.config.base_url}/api/generate"
        response = self._make_request(url, data)

        latency = (time.time() - start) * 1000

        return AIResponse(
            content=response.get("response", ""),
            model=self.config.model,
            provider="ollama",
            tokens_used=response.get("eval_count", 0),
            latency_ms=latency,
            raw_response=response,
        )

    def chat(self, messages: List[Message], **kwargs) -> AIResponse:
        start = time.time()

        data = {
            "model": self.config.model,
            "messages": [{"role": m.role, "content": m.content} for m in messages],
            "stream": False,
            "options": {
                "temperature": kwargs.get("temperature", self.config.temperature),
                "num_predict": kwargs.get("max_tokens", self.config.max_tokens),
            }
        }

        url = f"{self.config.base_url}/api/chat"
        response = self._make_request(url, data)

        latency = (time.time() - start) * 1000

        return AIResponse(
            content=response.get("message", {}).get("content", ""),
            model=self.config.model,
            provider="ollama",
            latency_ms=latency,
            raw_response=response,
        )

    def is_available(self) -> bool:
        try:
            url = f"{self.config.base_url}/api/tags"
            if isinstance(self.http_client, str):
                import urllib.request
                with urllib.request.urlopen(url, timeout=2) as resp:
                    return resp.status == 200
            else:
                resp = self.http_client.get(url, timeout=2)
                return resp.status_code == 200
        except Exception:
            return False


class OpenAIProvider(AIProvider):
    """OpenAI provider (cloud)."""

    def __init__(
        self,
        api_key: str = None,
        model: str = "gpt-4o-mini",
        base_url: str = "https://api.openai.com/v1",
        **kwargs
    ):
        config = ProviderConfig(
            provider_type=ProviderType.OPENAI,
            base_url=base_url.rstrip("/"),
            api_key=api_key or os.getenv("OPENAI_API_KEY", ""),
            model=model,
            **kwargs
        )
        super().__init__(config)

    def generate(
        self,
        prompt: str,
        context: str = None,
        system_prompt: str = None,
        **kwargs
    ) -> AIResponse:
        messages = []

        if system_prompt:
            messages.append(Message("system", system_prompt))
        if context:
            messages.append(Message("system", f"Context:\n{context}"))
        messages.append(Message("user", prompt))

        return self.chat(messages, **kwargs)

    def chat(self, messages: List[Message], **kwargs) -> AIResponse:
        start = time.time()

        data = {
            "model": self.config.model,
            "messages": [{"role": m.role, "content": m.content} for m in messages],
            "temperature": kwargs.get("temperature", self.config.temperature),
            "max_tokens": kwargs.get("max_tokens", self.config.max_tokens),
        }

        headers = {"Authorization": f"Bearer {self.config.api_key}"}
        url = f"{self.config.base_url}/chat/completions"
        response = self._make_request(url, data, headers)

        latency = (time.time() - start) * 1000

        return AIResponse(
            content=response["choices"][0]["message"]["content"],
            model=response.get("model", self.config.model),
            provider="openai",
            tokens_used=response.get("usage", {}).get("total_tokens", 0),
            latency_ms=latency,
            raw_response=response,
        )

    def is_available(self) -> bool:
        return bool(self.config.api_key)


class AnthropicProvider(AIProvider):
    """Anthropic/Claude provider (cloud)."""

    def __init__(
        self,
        api_key: str = None,
        model: str = "claude-3-haiku-20240307",
        **kwargs
    ):
        config = ProviderConfig(
            provider_type=ProviderType.ANTHROPIC,
            base_url="https://api.anthropic.com/v1",
            api_key=api_key or os.getenv("ANTHROPIC_API_KEY", ""),
            model=model,
            **kwargs
        )
        super().__init__(config)

    def generate(
        self,
        prompt: str,
        context: str = None,
        system_prompt: str = None,
        **kwargs
    ) -> AIResponse:
        messages = [Message("user", prompt)]

        system = system_prompt or ""
        if context:
            system += f"\n\nContext:\n{context}"

        return self.chat(messages, system=system, **kwargs)

    def chat(self, messages: List[Message], system: str = None, **kwargs) -> AIResponse:
        start = time.time()

        data = {
            "model": self.config.model,
            "messages": [{"role": m.role, "content": m.content} for m in messages],
            "max_tokens": kwargs.get("max_tokens", self.config.max_tokens),
        }

        if system:
            data["system"] = system

        headers = {
            "x-api-key": self.config.api_key,
            "anthropic-version": "2023-06-01",
        }

        url = f"{self.config.base_url}/messages"
        response = self._make_request(url, data, headers)

        latency = (time.time() - start) * 1000

        content = ""
        if response.get("content"):
            content = response["content"][0].get("text", "")

        return AIResponse(
            content=content,
            model=response.get("model", self.config.model),
            provider="anthropic",
            tokens_used=response.get("usage", {}).get("input_tokens", 0) +
                       response.get("usage", {}).get("output_tokens", 0),
            latency_ms=latency,
            raw_response=response,
        )

    def is_available(self) -> bool:
        return bool(self.config.api_key)


class OpenAICompatibleProvider(AIProvider):
    """Generic OpenAI-compatible provider."""

    def __init__(
        self,
        base_url: str,
        api_key: str = "",
        model: str = "",
        **kwargs
    ):
        config = ProviderConfig(
            provider_type=ProviderType.OPENAI_COMPATIBLE,
            base_url=base_url.rstrip("/"),
            api_key=api_key,
            model=model,
            **kwargs
        )
        super().__init__(config)

    def generate(
        self,
        prompt: str,
        context: str = None,
        system_prompt: str = None,
        **kwargs
    ) -> AIResponse:
        messages = []
        if system_prompt:
            messages.append(Message("system", system_prompt))
        if context:
            messages.append(Message("system", f"Context:\n{context}"))
        messages.append(Message("user", prompt))
        return self.chat(messages, **kwargs)

    def chat(self, messages: List[Message], **kwargs) -> AIResponse:
        start = time.time()

        data = {
            "messages": [{"role": m.role, "content": m.content} for m in messages],
            "temperature": kwargs.get("temperature", self.config.temperature),
            "max_tokens": kwargs.get("max_tokens", self.config.max_tokens),
            "stream": False,
        }

        if self.config.model:
            data["model"] = self.config.model

        headers = {}
        if self.config.api_key:
            headers["Authorization"] = f"Bearer {self.config.api_key}"

        url = f"{self.config.base_url}/chat/completions"
        response = self._make_request(url, data, headers)

        latency = (time.time() - start) * 1000

        return AIResponse(
            content=response["choices"][0]["message"]["content"],
            model=response.get("model", self.config.model or "unknown"),
            provider="openai_compatible",
            tokens_used=response.get("usage", {}).get("total_tokens", 0),
            latency_ms=latency,
            raw_response=response,
        )


# ==================== PROVIDER MANAGEMENT ====================

_default_provider: Optional[AIProvider] = None


def get_provider(
    provider_type: ProviderType = None,
    **kwargs
) -> Optional[AIProvider]:
    """Get an AI provider, auto-detecting if not specified.

    Priority:
    1. LM Studio (local, if running)
    2. Ollama (local, if running)
    3. OpenAI (if API key set)
    4. Anthropic (if API key set)

    Args:
        provider_type: Specific provider to use
        **kwargs: Provider-specific arguments

    Returns:
        AIProvider or None
    """
    global _default_provider

    if provider_type:
        return _create_provider(provider_type, **kwargs)

    # Auto-detect
    # 1. Try LM Studio
    lm_studio = LMStudioProvider(**kwargs)
    if lm_studio.is_available():
        print("[AI] Using LM Studio (local)")
        _default_provider = lm_studio
        return lm_studio

    # 2. Try Ollama
    ollama = OllamaProvider(**kwargs)
    if ollama.is_available():
        print("[AI] Using Ollama (local)")
        _default_provider = ollama
        return ollama

    # 3. Try OpenAI
    if os.getenv("OPENAI_API_KEY"):
        print("[AI] Using OpenAI")
        _default_provider = OpenAIProvider(**kwargs)
        return _default_provider

    # 4. Try Anthropic
    if os.getenv("ANTHROPIC_API_KEY"):
        print("[AI] Using Anthropic")
        _default_provider = AnthropicProvider(**kwargs)
        return _default_provider

    print("[AI] No provider available")
    return None


def _create_provider(provider_type: ProviderType, **kwargs) -> AIProvider:
    """Create a specific provider."""
    if provider_type == ProviderType.LM_STUDIO:
        return LMStudioProvider(**kwargs)
    elif provider_type == ProviderType.OLLAMA:
        return OllamaProvider(**kwargs)
    elif provider_type == ProviderType.OPENAI:
        return OpenAIProvider(**kwargs)
    elif provider_type == ProviderType.ANTHROPIC:
        return AnthropicProvider(**kwargs)
    elif provider_type == ProviderType.OPENAI_COMPATIBLE:
        return OpenAICompatibleProvider(**kwargs)
    else:
        raise ValueError(f"Unknown provider type: {provider_type}")


def list_available_providers() -> List[Dict[str, Any]]:
    """List all available providers."""
    providers = []

    # Check LM Studio
    lm = LMStudioProvider()
    providers.append({
        "type": "lm_studio",
        "name": "LM Studio",
        "local": True,
        "available": lm.is_available(),
        "url": "http://localhost:1234",
    })

    # Check Ollama
    ol = OllamaProvider()
    providers.append({
        "type": "ollama",
        "name": "Ollama",
        "local": True,
        "available": ol.is_available(),
        "url": "http://localhost:11434",
    })

    # Check OpenAI
    providers.append({
        "type": "openai",
        "name": "OpenAI",
        "local": False,
        "available": bool(os.getenv("OPENAI_API_KEY")),
        "url": "https://api.openai.com",
    })

    # Check Anthropic
    providers.append({
        "type": "anthropic",
        "name": "Anthropic",
        "local": False,
        "available": bool(os.getenv("ANTHROPIC_API_KEY")),
        "url": "https://api.anthropic.com",
    })

    return providers
