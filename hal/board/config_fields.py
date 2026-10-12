"""Unknown keys in a device's board config: logged and dropped, never fatal."""

import logging
from typing import Iterable

logger = logging.getLogger(__name__)


def known_fields(entry: dict, allowed: Iterable[str], where: str) -> dict:
    """`entry` without the keys this HAL does not know, each named in a warning.

    The device package and HAL update separately, so a newer package can carry a
    field an older HAL has never seen. Failing boot on it took HAL down on lamp-52e6
    (device 0.0.109 with HAL 0.1.282, `contact.release_after_s`). A typo still shows
    up in the log, and the field it misspells keeps its default.
    """
    allowed = set(allowed)
    unknown = sorted(set(entry) - allowed)
    if unknown:
        logger.warning("%s: ignoring fields this HAL does not know: %s", where, unknown)
    return {key: value for key, value in entry.items() if key in allowed}
