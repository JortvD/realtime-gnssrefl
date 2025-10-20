from flask import Flask, request
import os
import base64
import struct
import json

# Measurement and scaling constants
MAX_STANDARD_DEVIATION: float = 1.0
MAX_RELATIVE_HEIGHT: float = 20.0

# Temperature scaling range (Celsius)
MIN_TEMPERATURE_C: float = -30.0
MAX_TEMPERATURE_C: float = 80.0

# Battery voltage range (millivolts)
MIN_BATTERY_MV: int = 2000
MAX_BATTERY_MV: int = 5000

def map_u8_to_float(byte_val: int, min_val: float, max_val: float) -> float:
    if byte_val <= 0:
        return min_val
    if byte_val >= 255:
        return max_val
    return min_val + (byte_val / 255.0) * (max_val - min_val)

def map_u16_to_float(u16_val: int, min_val: float, max_val: float) -> float:
    if u16_val <= 0:
        return min_val
    if u16_val >= 65535:
        return max_val
    return min_val + (u16_val / 65535.0) * (max_val - min_val)

def map_u8_to_int(byte_val: int, min_val: int, max_val: int) -> int:
    if byte_val <= 0:
        return min_val
    if byte_val >= 255:
        return max_val
    scaled = byte_val / 255.0
    return int(round(min_val + scaled * (max_val - min_val)))

MESSAGES_FOLDER = 'messages'
STATIC_FOLDER = 'static'
os.makedirs(MESSAGES_FOLDER, exist_ok=True)

app = Flask(__name__, static_folder=STATIC_FOLDER)

@app.route('/IBjExH3CB1b@', methods=['POST'])
def webhook():
    data = request.get_json()
    print(data)
    message_b64 = data.get('message', [])
    packet_bytes = base64.b64decode(str(message_b64))
    HEADER_FORMAT = "<BBBB"           # battery, temp, lat, lon (4 bytes)
    MEASUREMENT_FORMAT = "<BHBBB"     # u8, u16 (LE), u8, u8, u8 (6 bytes)
    HEADER_SIZE = struct.calcsize(HEADER_FORMAT)
    MEASUREMENT_SIZE = struct.calcsize(MEASUREMENT_FORMAT)

    # Parse header
    header_data = packet_bytes[:HEADER_SIZE]
    battery_raw, temp_raw, lat, lon = struct.unpack(HEADER_FORMAT, header_data)

    battery = map_u8_to_int(battery_raw, MIN_BATTERY_MV, MAX_BATTERY_MV)
    temp = map_u8_to_float(temp_raw, MIN_TEMPERATURE_C, MAX_TEMPERATURE_C)

    measurements_bytes = packet_bytes[HEADER_SIZE:]
    measurements = []

    for i in range(0, len(measurements_bytes), MEASUREMENT_SIZE):
        chunk = measurements_bytes[i:i+MEASUREMENT_SIZE]
        if len(chunk) < MEASUREMENT_SIZE:
            break  # stop if incomplete
        uid, raw_rel_mean, raw_rel_std, num_used, num_seen = struct.unpack(MEASUREMENT_FORMAT, chunk)
        rel_mean = map_u16_to_float(raw_rel_mean, 0, MAX_RELATIVE_HEIGHT)
        rel_std = map_u8_to_float(raw_rel_std, 0, MAX_STANDARD_DEVIATION)

        measurements.append({
            "uid": uid,
            "rh": rel_mean,
            "std": rel_std,
            "n_used": num_used,
            "n_seen": num_seen
        })

    message = {
        "battery": battery,
        "temperature": temp,
        "latitude": lat,
        "longitude": lon,
        "measurements": measurements
    }

    data['message'] = message
    filename = os.path.join(MESSAGES_FOLDER, f"{data.get('id', 'unknown')}.json")
    with open(filename, 'w') as f:
        json.dump(data, f)

    return '', 204

@app.route('/', methods=['GET'])
def index():
    return app.send_static_file('index.html')

@app.route('/data', methods=['GET'])
def data():
    all_measurements = []
    for filename in os.listdir(MESSAGES_FOLDER):
        if filename.endswith('.json'):
            filepath = os.path.join(MESSAGES_FOLDER, filename)
            with open(filepath, 'r') as f:
                try:
                    message = json.load(f)
                    all_measurements.append(message)
                except json.JSONDecodeError:
                    continue
    return all_measurements

if __name__ == '__main__':
    app.run(port=5025)