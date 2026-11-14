"""Inspect the archived portable EXE without importing Windows-only tooling."""
import json
import struct
import sys
from pathlib import Path

binary = Path(sys.argv[1]).read_bytes()
pe = struct.unpack_from('<I', binary, 0x3c)[0]
assert binary[pe:pe + 4] == b'PE\0\0', 'PE signature missing'
assert struct.unpack_from('<H', binary, pe + 4)[0] == 0x8664, 'Expected x64 machine'
assert struct.unpack_from('<H', binary, pe + 24)[0] == 0x20b, 'Expected PE32+'
assert struct.unpack_from('<H', binary, pe + 24 + 68)[0] == 2, 'Expected GUI subsystem'
assert struct.unpack_from('<I', binary, pe + 24 + 112 + 16)[0] != 0, 'Icon/manifest resources missing'
sections = pe + 24 + struct.unpack_from('<H', binary, pe + 20)[0]
ranges = []
for index in range(struct.unpack_from('<H', binary, pe + 6)[0]):
    size, address, raw_size, offset = struct.unpack_from('<IIII', binary, sections + index * 40 + 8)
    ranges.append((address, max(size, raw_size), offset))


def locate(address):
    for start, size, offset in ranges:
        if start <= address < start + size:
            return offset + address - start
    raise ValueError(f'RVA outside sections: {address:x}')


imports = []
entry = locate(struct.unpack_from('<I', binary, pe + 24 + 112 + 8)[0])
while any(binary[entry:entry + 20]):
    name = locate(struct.unpack_from('<I', binary, entry + 12)[0])
    imports.append(binary[name:binary.index(b'\0', name)].decode('ascii'))
    entry += 20
assert not any(name.lower().startswith(('vcruntime', 'msvcp')) for name in imports), 'Portable EXE requires an extra Visual C++ runtime'
print(json.dumps({'machine': 'x64', 'subsystem': 'GUI', 'resources': True, 'extraVisualCppRuntime': False, 'imports': sorted(set(imports))}))
