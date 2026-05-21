import os
from PIL import Image


VIDEO_EXTENSIONS = {
    '.mp4', '.mkv', '.avi', '.mov', '.wmv', '.flv', '.webm',
    '.m4v', '.mpg', '.mpeg', '.3gp', '.ogv', '.ts', '.mts', '.m2ts', '.vob'
}


def is_image_file(filepath):
    """Check if a file is a valid image by attempting PIL open + verify."""
    try:
        with Image.open(filepath) as img:
            img.verify()
        return True
    except Exception:
        return False


def is_video_file(filepath):
    """Check if a file has a known video extension."""
    ext = os.path.splitext(filepath)[1].lower()
    return ext in VIDEO_EXTENSIONS
