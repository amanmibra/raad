"""Stable errors returned by the Rust core."""
import json


class RaadError(Exception):
    code = "unknown"

    def __init__(self, message: str, *, code: str | None = None):
        super().__init__(message)
        if code is not None:
            self.code = code


class InvalidPathError(RaadError):
    code = "invalid_path"


class NotFoundError(RaadError):
    code = "not_found"


class CacheMissError(RaadError):
    code = "cache_miss"


class CacheCorruptionError(RaadError):
    code = "cache_corruption"


class ValidationError(RaadError):
    code = "validation"


class PermissionDeniedError(RaadError):
    code = "permission_denied"


class ConflictError(RaadError):
    code = "conflict"


class RemoteUnavailableError(RaadError):
    code = "remote_unavailable"


class RateLimitError(RaadError):
    code = "rate_limit"


class LimitError(RaadError):
    code = "limit"


class ConfigurationError(RaadError):
    code = "configuration"


_ERRORS = {cls.code: cls for cls in (
    InvalidPathError, NotFoundError, CacheMissError, CacheCorruptionError,
    ValidationError, PermissionDeniedError, ConflictError, RemoteUnavailableError,
    RateLimitError, LimitError, ConfigurationError,
)}


def native_error(error: RuntimeError, token: str | None = None) -> RaadError:
    try:
        value = json.loads(str(error))
        code, message = value["code"], value["message"]
        if not isinstance(code, str) or not isinstance(message, str):
            raise ValueError
    except (ValueError, KeyError, TypeError):
        return RaadError("native raad operation failed")
    if token:
        message = message.replace(token, "[redacted]")
    return _ERRORS.get(code, RaadError)(message, code=code)
