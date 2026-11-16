"""Reject mismatched PNG dimensions in ICNS entries before signing a release."""
import struct
import sys
from pathlib import Path

expected = {'icp4': 16, 'icp5': 32, 'icp6': 64, 'ic07': 128,
            'ic08': 256, 'ic09': 512, 'ic10': 1024, 'ic11': 32,
            'ic12': 64, 'ic13': 256, 'ic14': 512}
data = Path(sys.argv[1]).read_bytes()
assert data[:4] == b'icns' and struct.unpack_from('>I', data, 4)[0] == len(data)
offset = 8
seen = set()
while offset < len(data):
    name = data[offset:offset + 4].decode('ascii')
    size = struct.unpack_from('>I', data, offset + 4)[0]
    assert size >= 8 and offset + size <= len(data), 'Invalid ICNS chunk'
    payload = data[offset + 8:offset + size]
    if name in expected and payload.startswith(b'\x89PNG'):
        dimensions = struct.unpack_from('>II', payload, 16)
        assert dimensions == (expected[name], expected[name]), f'{name}: {dimensions}, expected {expected[name]}'
    seen.add(name)
    offset += size
assert offset == len(data)
assert 'ic10' in seen, '1024-pixel representation missing'
print('Apple ICNS representations verified:', ', '.join(sorted(seen)))
