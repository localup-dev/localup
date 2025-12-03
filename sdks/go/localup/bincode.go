package localup

import (
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"math"
)

// BincodeEncoder encodes values in bincode format.
// This is compatible with Rust's bincode serialization.
type BincodeEncoder struct {
	buf *bytes.Buffer
}

// NewBincodeEncoder creates a new bincode encoder.
func NewBincodeEncoder() *BincodeEncoder {
	return &BincodeEncoder{
		buf: new(bytes.Buffer),
	}
}

// Bytes returns the encoded bytes.
func (e *BincodeEncoder) Bytes() []byte {
	return e.buf.Bytes()
}

// Reset clears the encoder buffer.
func (e *BincodeEncoder) Reset() {
	e.buf.Reset()
}

// WriteU8 writes a uint8.
func (e *BincodeEncoder) WriteU8(v uint8) {
	e.buf.WriteByte(v)
}

// WriteU16 writes a uint16 in little-endian.
func (e *BincodeEncoder) WriteU16(v uint16) {
	var buf [2]byte
	binary.LittleEndian.PutUint16(buf[:], v)
	e.buf.Write(buf[:])
}

// WriteU32 writes a uint32 in little-endian.
func (e *BincodeEncoder) WriteU32(v uint32) {
	var buf [4]byte
	binary.LittleEndian.PutUint32(buf[:], v)
	e.buf.Write(buf[:])
}

// WriteU64 writes a uint64 in little-endian.
func (e *BincodeEncoder) WriteU64(v uint64) {
	var buf [8]byte
	binary.LittleEndian.PutUint64(buf[:], v)
	e.buf.Write(buf[:])
}

// WriteBool writes a boolean as a single byte.
func (e *BincodeEncoder) WriteBool(v bool) {
	if v {
		e.WriteU8(1)
	} else {
		e.WriteU8(0)
	}
}

// WriteString writes a string with length prefix.
func (e *BincodeEncoder) WriteString(s string) {
	e.WriteU64(uint64(len(s)))
	e.buf.WriteString(s)
}

// WriteBytes writes a byte slice with length prefix.
func (e *BincodeEncoder) WriteBytes(data []byte) {
	e.WriteU64(uint64(len(data)))
	e.buf.Write(data)
}

// WriteOption writes an optional value.
// Tag 0 = None, Tag 1 = Some.
func (e *BincodeEncoder) WriteOptionU16(v *uint16) {
	if v == nil {
		e.WriteU8(0) // None
	} else {
		e.WriteU8(1) // Some
		e.WriteU16(*v)
	}
}

// WriteOptionString writes an optional string.
func (e *BincodeEncoder) WriteOptionString(v *string) {
	if v == nil {
		e.WriteU8(0) // None
	} else {
		e.WriteU8(1) // Some
		e.WriteString(*v)
	}
}

// WriteOptionBytes writes an optional byte slice.
func (e *BincodeEncoder) WriteOptionBytes(v []byte) {
	if v == nil {
		e.WriteU8(0) // None
	} else {
		e.WriteU8(1) // Some
		e.WriteBytes(v)
	}
}

// WriteVec writes a vector with length prefix.
func (e *BincodeEncoder) WriteVecLen(length int) {
	e.WriteU64(uint64(length))
}

// BincodeDecoder decodes values from bincode format.
type BincodeDecoder struct {
	r   io.Reader
	buf []byte
}

// NewBincodeDecoder creates a new bincode decoder.
func NewBincodeDecoder(r io.Reader) *BincodeDecoder {
	return &BincodeDecoder{
		r:   r,
		buf: make([]byte, 8),
	}
}

// NewBincodeDecoderBytes creates a decoder from a byte slice.
func NewBincodeDecoderBytes(data []byte) *BincodeDecoder {
	return NewBincodeDecoder(bytes.NewReader(data))
}

// ReadU8 reads a uint8.
func (d *BincodeDecoder) ReadU8() (uint8, error) {
	if _, err := io.ReadFull(d.r, d.buf[:1]); err != nil {
		return 0, err
	}
	return d.buf[0], nil
}

// ReadU16 reads a uint16 in little-endian.
func (d *BincodeDecoder) ReadU16() (uint16, error) {
	if _, err := io.ReadFull(d.r, d.buf[:2]); err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint16(d.buf[:2]), nil
}

// ReadU32 reads a uint32 in little-endian.
func (d *BincodeDecoder) ReadU32() (uint32, error) {
	if _, err := io.ReadFull(d.r, d.buf[:4]); err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint32(d.buf[:4]), nil
}

// ReadU64 reads a uint64 in little-endian.
func (d *BincodeDecoder) ReadU64() (uint64, error) {
	if _, err := io.ReadFull(d.r, d.buf[:8]); err != nil {
		return 0, err
	}
	return binary.LittleEndian.Uint64(d.buf[:8]), nil
}

// ReadBool reads a boolean.
func (d *BincodeDecoder) ReadBool() (bool, error) {
	v, err := d.ReadU8()
	if err != nil {
		return false, err
	}
	return v != 0, nil
}

// ReadString reads a length-prefixed string.
func (d *BincodeDecoder) ReadString() (string, error) {
	length, err := d.ReadU64()
	if err != nil {
		return "", err
	}
	if length > math.MaxInt32 {
		return "", errors.New("string too long")
	}
	buf := make([]byte, length)
	if _, err := io.ReadFull(d.r, buf); err != nil {
		return "", err
	}
	return string(buf), nil
}

// ReadBytes reads a length-prefixed byte slice.
func (d *BincodeDecoder) ReadBytes() ([]byte, error) {
	length, err := d.ReadU64()
	if err != nil {
		return nil, err
	}
	if length > MaxFrameSize {
		return nil, errors.New("bytes too long")
	}
	buf := make([]byte, length)
	if _, err := io.ReadFull(d.r, buf); err != nil {
		return nil, err
	}
	return buf, nil
}

// ReadOptionU16 reads an optional uint16.
func (d *BincodeDecoder) ReadOptionU16() (*uint16, error) {
	tag, err := d.ReadU8()
	if err != nil {
		return nil, err
	}
	if tag == 0 {
		return nil, nil // None
	}
	v, err := d.ReadU16()
	if err != nil {
		return nil, err
	}
	return &v, nil
}

// ReadOptionString reads an optional string.
func (d *BincodeDecoder) ReadOptionString() (*string, error) {
	tag, err := d.ReadU8()
	if err != nil {
		return nil, err
	}
	if tag == 0 {
		return nil, nil // None
	}
	s, err := d.ReadString()
	if err != nil {
		return nil, err
	}
	return &s, nil
}

// ReadOptionBytes reads an optional byte slice.
func (d *BincodeDecoder) ReadOptionBytes() ([]byte, error) {
	tag, err := d.ReadU8()
	if err != nil {
		return nil, err
	}
	if tag == 0 {
		return nil, nil // None
	}
	return d.ReadBytes()
}

// ReadVecLen reads the length of a vector.
func (d *BincodeDecoder) ReadVecLen() (uint64, error) {
	return d.ReadU64()
}
