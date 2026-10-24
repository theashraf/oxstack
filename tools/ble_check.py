"""Exercise the `ble_peripheral` example from a computer.

Flash it (`cargo run --release --bin ble_peripheral --features ble` in
examples/cardputer), then run:

    uv run --with bleak tools/ble_check.py

It finds "oxstack", lists its GATT table, reads the battery level, cycles the
RGB LED through red/green/blue/off (reading each write back), then waits up
to 120 s for 5 key notifications typed on the Cardputer. On macOS the first
run may ask for Bluetooth permission for your terminal.
"""
import asyncio

from bleak import BleakClient, BleakScanner

SVC = "4f58a000-6b1f-4d8e-9a3b-3c1e5a7d2b10"
KEY = "4f58a001-6b1f-4d8e-9a3b-3c1e5a7d2b10"
LED = "4f58a002-6b1f-4d8e-9a3b-3c1e5a7d2b10"
BATTERY_LEVEL = "00002a19-0000-1000-8000-00805f9b34fb"


async def main() -> None:
    print("scanning for 'oxstack'...")
    dev = await BleakScanner.find_device_by_name("oxstack", timeout=20)
    if dev is None:
        print("FAIL: not found")
        return
    print(f"PASS found {dev.name} ({dev.address})")
    async with BleakClient(dev) as c:
        print("PASS connected")
        for s in c.services:
            print(f"  service {s.uuid}")
            for ch in s.characteristics:
                print(f"    char {ch.uuid} {ch.properties}")
        print(f"PASS battery level {(await c.read_gatt_char(BATTERY_LEVEL))[0]}%")

        for rgb in [(32, 0, 0), (0, 32, 0), (0, 0, 32), (0, 0, 0)]:
            await c.write_gatt_char(LED, bytes(rgb), response=True)
            back = tuple(await c.read_gatt_char(LED))
            print(f"{'PASS' if back == rgb else 'FAIL'} LED write {rgb} -> read back {back}")
            await asyncio.sleep(1.5)

        keys: list[str] = []
        await c.start_notify(KEY, lambda _, d: (keys.append(chr(d[0])), print(f"  key notify: {chr(d[0])!r}")))
        print(">>> TYPE ON THE CARDPUTER NOW (up to 120 s, stops after 5 keys) <<<")
        for _ in range(240):
            if len(keys) >= 5:
                break
            await asyncio.sleep(0.5)
        await c.stop_notify(KEY)
        print(f"{'PASS' if keys else 'INFO no'} key notifications: {''.join(keys)!r}")


asyncio.run(main())
