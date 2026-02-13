/**
 * Breakpoint wire protocol helpers.
 *
 * Wire format: [1-byte MessageType] [msgpack payload]
 * rmp_serde encodes Rust structs as msgpack arrays (not maps).
 */
import { pack, unpack } from 'msgpackr';

// Message type discriminators (must match Rust MessageType repr(u8))
export const MSG = {
  // Client -> Server
  PLAYER_INPUT:      0x01,
  JOIN_ROOM:         0x02,
  LEAVE_ROOM:        0x03,
  CLAIM_ALERT:       0x04,
  CHAT_MESSAGE:      0x05,
  // Server -> Client
  JOIN_ROOM_RESPONSE:0x06,
  // Host -> Client
  GAME_STATE:        0x10,
  PLAYER_LIST:       0x11,
  ROOM_CONFIG:       0x12,
  GAME_START:        0x13,
  ROUND_END:         0x14,
  GAME_END:          0x15,
  // Alert
  ALERT_EVENT:       0x20,
  ALERT_CLAIMED:     0x21,
  ALERT_DISMISSED:   0x22,
  OVERLAY_CONFIG:    0x23,
};

/**
 * Encode a message to wire format.
 * @param {number} type - Message type byte
 * @param {any} payload - Payload to encode (will be packed as msgpack)
 * @returns {Buffer}
 */
export function encode(type, payload) {
  const payloadBuf = pack(payload);
  const buf = Buffer.alloc(1 + payloadBuf.length);
  buf[0] = type;
  payloadBuf.copy(buf, 1);
  return buf;
}

/**
 * Decode a wire message.
 * @param {Buffer|Uint8Array} data - Raw wire data
 * @returns {{ type: number, payload: any }}
 */
export function decode(data) {
  const buf = Buffer.from(data);
  const type = buf[0];
  const payload = unpack(buf.subarray(1));
  return { type, payload };
}

/**
 * Build a JoinRoom client message.
 * JoinRoomMsg struct fields (array order): [room_code, player_name, player_color]
 * PlayerColor struct fields: [r, g, b]
 */
export function joinRoomMsg(roomCode, playerName, color = [128, 200, 255]) {
  return encode(MSG.JOIN_ROOM, [roomCode, playerName, color]);
}

/**
 * Build a GameStart server message.
 * GameStartMsg fields: [game_name, players, host_id]
 * Player fields: [id, display_name, [r,g,b], is_host, is_spectator]
 */
export function gameStartMsg(gameName, players, hostId) {
  return encode(MSG.GAME_START, [gameName, players, hostId]);
}

/**
 * Parse a JoinRoomResponse payload.
 * Fields: [success, player_id, room_code, room_state, error]
 */
export function parseJoinRoomResponse(payload) {
  return {
    success: payload[0],
    playerId: payload[1],
    roomCode: payload[2],
    roomState: payload[3],
    error: payload[4],
  };
}

/**
 * Parse a PlayerList payload.
 * Fields: [players, host_id]
 * Each player: [id, display_name, [r,g,b], is_host, is_spectator]
 */
export function parsePlayerList(payload) {
  return {
    players: payload[0],
    hostId: payload[1],
  };
}

/**
 * Parse a GameStart payload.
 * Fields: [game_name, players, host_id]
 */
export function parseGameStart(payload) {
  return {
    gameName: payload[0],
    players: payload[1],
    hostId: payload[2],
  };
}

/**
 * Human-readable message type name.
 */
export function msgTypeName(type) {
  for (const [name, val] of Object.entries(MSG)) {
    if (val === type) return name;
  }
  return `UNKNOWN(0x${type.toString(16)})`;
}
