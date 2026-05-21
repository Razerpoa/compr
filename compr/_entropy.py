import math
from collections import Counter


def calculate_entropy(data_bytes):
    """Calculates standard Shannon Entropy (0 to 8 bits/byte)"""
    if not data_bytes:
        return 0.0
    length = len(data_bytes)
    counter = Counter(data_bytes)
    entropy = 0.0
    for count in counter.values():
        p = count / length
        entropy -= p * math.log2(p)
    return entropy
