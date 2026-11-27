"""Build original emulator test tones. Run with Python 3 from any directory.

All instruction sequences, register settings and waveforms below are authored
for these tests. The generator and its outputs are available under MIT.
"""
import gzip
from io import BytesIO
from pathlib import Path
from struct import pack, pack_into

ASSETS = Path(__file__).resolve().parents[1] / "assets"


def le16(n):
    return pack("<H", n)


def writes6502(registers):
    """LDA #value / STA address for each sound register."""
    return b"".join(bytes([0xA9, value, 0x8D]) + le16(addr) for addr, value in registers)


def nes(chip=0):
    # Remember A (track) and enable two square waves with distinct duty cycles.
    init = bytes.fromhex("85 00") + writes6502([
        (0x4015, 3), (0x4000, 0xBA), (0x4002, 0x90), (0x4003, 1),
        (0x4004, 0x75), (0x4006, 0x63), (0x4007, 2),
    ])
    if chip == 1:  # VRC6 pulses and saw
        init += writes6502([
            (0x9000, 0x39), (0x9001, 0x80), (0x9002, 0x81),
            (0xA000, 0x26), (0xA001, 0x48), (0xA002, 0x82),
            (0xB000, 0x19), (0xB001, 0xB0), (0xB002, 0x81),
        ])
    if chip == 4:  # FDS wave RAM, volume, disable modulation, pitch
        wave = [(0x4040 + i, i if i < 32 else 63 - i) for i in range(64)]
        init += writes6502([(0x4023, 2), (0x4089, 0x80)] + wave + [
            (0x4089, 0), (0x4080, 0x9F), (0x4087, 0x80),
            (0x4082, 0xA1), (0x4083, 1),
        ])
    if chip == 32:  # Sunsoft 5B address latch and data port
        for reg, value in enumerate([0x51, 1, 0x75, 1, 0x91, 2, 0, 0x38, 8, 9, 10]):
            init += writes6502([(0xC000, reg), (0xE000, value)])
    # RTS; play increments the track-dependent period once per frame.
    play = bytes.fromhex("E6 00 A5 00 09 40 8D 02 40 60")
    play_addr = 0x8000 + len(init) + 1
    program = init + b"\x60" + play
    header = bytearray(128)
    header[:8] = b"NESM\x1a\x01\x01\x01"
    pack_into("<3H", header, 8, 0x8000, 0x8000, play_addr)
    header[14:25] = b"Pulse sweep"
    header[78:81] = b"MIT"
    pack_into("<H", header, 110, 16666)
    pack_into("<H", header, 120, 20000)
    header[123] = chip
    header[124] = 0x82  # Reserved byte must never enable an expansion chip.

    def chunk(tag, data):
        return pack("<I", len(data)) + tag + data

    info = header[8:14] + bytes([0, chip, 1, 0])
    nsfe = (b"NSFE" + chunk(b"INFO", info) + chunk(b"DATA", program)
            + chunk(b"tlbl", b"Sweep\0") + chunk(b"time", pack("<I", 76000))
            + chunk(b"fade", pack("<I", 2000)) + chunk(b"NEND", b""))
    return bytes(header) + program, nsfe


def z80_psg(spectrum):
    """Initialize AY channels A and B via Spectrum or MSX ports."""
    code = bytearray()
    for reg, val in [(0, 0x81), (1, 1), (2, 0x91), (3, 2), (7, 0x3C), (8, 12), (9, 10)]:
        if spectrum:  # LD BC,port / LD A,n / OUT (C),A
            for port, value in [(0xFFFD, reg), (0xBFFD, val)]:
                code += b"\x01" + le16(port) + bytes([0x3E, value, 0xED, 0x79])
        else:  # LD A,n / OUT (n),A
            code += bytes([0x3E, reg, 0xD3, 0xA0, 0x3E, val, 0xD3, 0xA1])
    return code + b"\xc9"


def ay():
    program = z80_psg(True)
    header = bytearray(54)
    header[:8] = b"ZXAYEMUL"
    # Big-endian pointers are relative to the address of each field.
    for field, target in [(12, 52), (14, 52), (18, 20), (20, 52), (22, 24),
                          (34, 38), (36, 44), (48, 54)]:
        pack_into(">H", header, field, target - field)
    pack_into(">3H", header, 38, 0xF000, 0x8000, 0x8000 + len(program))
    pack_into(">2H", header, 44, 0x8000, len(program) + 1)
    return header + program + b"\xc9"  # empty interrupt handler


def kss():
    init = z80_psg(False)
    return b"KSCC" + pack("<4H4x", 0x8000, len(init) + 1, 0x8000, 0x8000 + len(init)) + init + b"\xc9"


def gbs():
    init = bytearray()
    for reg, val in [(0x26, 0x80), (0x24, 0x77), (0x25, 0x11),
                     (0x11, 0x80), (0x12, 0xC0), (0x13, 0x30), (0x14, 0x86)]:
        init += bytes([0x3E, val, 0xE0, reg])  # LD A,n / LDH (reg),A
    header = bytearray(0x70)
    header[:6] = b"GBS\x01\x01\x01"
    pack_into("<4H", header, 6, 0x4000, 0x4000, 0x4000 + len(init) + 1, 0xDFF0)
    return header + init + b"\xc9\xc9"


def hes():
    # MPR0 maps sound I/O, MPR1 RAM, MPR2 the program ROM at $4000.
    # Track 0 is intentionally silent; other tracks set a distinct period.
    code = bytes.fromhex("C9 00 D0 01 60 AA")  # CMP / BNE / RTS / TAX
    code += writes6502([(0x800, 0), (0x801, 0xFF), (0x804, 0), (0x805, 0xFF)])
    code += writes6502([(0x806, (i * 3) % 32) for i in range(32)])
    code += bytes.fromhex("8A 0A 09 80 8D 02 08")  # TXA / ASL / ORA / STA
    code += writes6502([(0x803, 1), (0x804, 0x9F)]) + b"\x60"
    return (b"HESM\0\0" + le16(0x4000) + bytes([0xFF, 0xF8, 0, 1, 2, 3, 4, 5])
            + b"DATA" + pack("<I8x", len(code)) + code)


def sap(multitrack):
    start = 0x2000
    # Type B takes track in A; type C takes it in X at PLAYER+3.
    init = (b"" if multitrack else b"\x8a") + bytes.fromhex("85 80")
    init += writes6502([(0xD201, 0xAA), (0xD203, 0xA5), (0xD202, 0xBE)]) + b"\x60"
    play = bytes.fromhex("E6 80 A5 80 09 40 8D 00 D2 60")
    if multitrack:
        desc = f"TYPE B\r\nSONGS 2\r\nINIT {start:04X}\r\nPLAYER {start + len(init):04X}"
        program = init + play
    else:
        desc = "TYPE C\r\nMUSIC 2000\r\nPLAYER 2000"
        program = bytes(3) + b"\x4c" + le16(start + 9) + b"\x4c" + le16(start + 9 + len(init)) + init + play
    return (f'SAP\r\nNAME "Pulse sweep"\r\n{desc}\r\n'.encode()
            + b"\xff\xff" + pack("<HH", start, start + len(program) - 1) + program)


def spc(alternate):
    header = bytearray(256)
    header[:33] = b"SNES-SPC700 Sound File Data v0.30"
    header[33:37] = bytes([26, 26, 27, 30])
    pack_into("<H", header, 37, 0x200)
    header[43] = 0xEF  # stack pointer
    ram = bytearray(65536)
    dsp = bytearray(128)
    dsp[0x0C] = dsp[0x1C] = 0x65  # master volumes
    dsp[0x5D], dsp[0x6C] = 0x10, 0x20  # sample directory, disable echo writes
    ram[0x1000:0x1004] = pack("<HH", 0x1100, 0x1100)
    # Original, looping 16-sample BRR waveform (filter 0).
    ram[0x1100:0x1109] = bytes([0xB3, 0x12, 0x45, 0x67, 0x54, 0x20, 0xED, 0xAB, 0xCD])
    for voice in range(8):
        base = voice * 16
        dsp[base:base + 8] = bytes([16 + voice * 2, 31 - voice, 0x80,
                                    2 + voice + int(alternate), 0, 0, 0, 100])
    dsp[0x4C] = 0xFF
    # MOV dp,#imm selects KON then keys all voices. Repeatedly write a
    # changing pitch after a delay, exercising the CPU and DSP write trace.
    code = bytes.fromhex("8F 4C F2 8F FF F3 8F 02 F2 E8 20 CD FF 1D D0 FD BC C4 F3 2F F6")
    ram[0x200:0x200 + len(code)] = code
    return header + ram + dsp + bytes(128)


def sega():
    # Same FM register writes encoded as a GYM log and as VGM commands.
    fm = [(0xB0, 7), (0xB4, 0xC0)]
    for op in [0, 4, 8, 12]:
        fm += [(0x30 + op, 1), (0x40 + op, 0x18), (0x50 + op, 0x1F), (0x80 + op, 0x0F)]
    fm += [(0xA4, 0x22), (0xA0, 0x80), (0x28, 0xF0)]
    gym = bytearray(b"GYMX" + bytes(424))
    commands = bytearray()
    for reg, val in fm:
        gym += bytes([1, reg, val])
        commands += bytes([0x52, reg, val])
    psg = bytes.fromhex("50 84 50 12 50 94 50 A8 50 1A 50 B8")

    def vgm(stream, clock):
        header = bytearray(64)
        header[:4] = b"Vgm "
        pack_into("<3I", header, 4, 60 + len(stream), 0x150, 3579545)
        pack_into("<3I", header, 0x18, 735 * 120, 64 - 0x1C, 735 * 120)
        pack_into("<I", header, 0x2C, clock)
        pack_into("<I", header, 0x34, 12)
        return header + stream

    wait = b"\x62" * 120 + b"\x66"
    # Sustain the GYM tone for 3600 frames.
    gym += bytes(3600)
    compressed = BytesIO()
    with gzip.GzipFile(fileobj=compressed, mode="wb", mtime=0, compresslevel=9) as out:
        out.write(vgm(commands + psg + wait, 7670454))
    return vgm(psg + wait, 0), compressed.getvalue(), gym


def main():
    nsf, nsfe = nes()
    vgm, vgz, gym = sega()
    outputs = {
        "test.nsf": nsf, "test.nsfe": nsfe, "test.ay": ay(), "test.kss": kss(),
        "test.gbs": gbs(), "test.hes": hes(), "test.sap": sap(False),
        "two_tracks.sap": sap(True), "test.spc": spc(False), "eight_voices.spc": spc(True),
        "test.vgm": vgm, "test.vgz": vgz, "test.gym": gym,
        "expansion/vrc6.nsf": nes(1)[0], "expansion/fds.nsf": nes(4)[0],
        "expansion/fme7.nsfe": nes(32)[1],
    }
    for name, data in outputs.items():
        (ASSETS / name).write_bytes(data)


if __name__ == "__main__":
    main()
