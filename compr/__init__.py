"""compr — Streaming solid archive compression for images and videos."""

from compr._classify import is_image_file, is_video_file, VIDEO_EXTENSIONS
from compr._packer import pack_to_stream
from compr._unpacker import unpack_from_stream
from compr._archive import build_solid_archive, extract_solid_archive
from compr._entropy import calculate_entropy

__all__ = [
    "is_image_file",
    "is_video_file",
    "VIDEO_EXTENSIONS",
    "pack_to_stream",
    "unpack_from_stream",
    "build_solid_archive",
    "extract_solid_archive",
    "calculate_entropy",
]
