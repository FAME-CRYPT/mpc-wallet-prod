package main

import (
	"sync"
	"time"
)

// SigningRequest represents a request to create a threshold signature
// It tracks the message to sign, current status, and the final signature
type SigningRequest struct {
	// ID uniquely identifies this signing request
	ID string `json:"id"`
	// Message is the data to be signed
	Message string `json:"message"`
	// Status indicates the current state: "pending", "in_progress", "completed", or "failed"
	Status string `json:"status"`
	// Signature contains the final threshold signature (only set when Status is "completed")
	Signature string `json:"signature,omitempty"`
	// CreatedAt is when this request was created
	CreatedAt time.Time `json:"created_at"`
	// UpdatedAt is when this request was last modified
	UpdatedAt time.Time `json:"updated_at"`
}

// NodeMessage represents a message sent between nodes during the signing protocol
// Nodes use this to exchange cryptographic data needed for threshold signing
type NodeMessage struct {
	// ID uniquely identifies this message
	ID string `json:"id"`
	// RequestID links this message to a specific signing request
	RequestID string `json:"request_id"`
	// FromNode is the identifier of the node that sent this message
	FromNode string `json:"from_node"`
	// ToNode is the identifier of the intended recipient ("*" for broadcast)
	ToNode string `json:"to_node"`
	// Round indicates which round of the protocol this message belongs to
	Round int `json:"round"`
	// Payload contains the cryptographic data (JSON-encoded)
	Payload string `json:"payload"`
	// CreatedAt is when this message was posted
	CreatedAt time.Time `json:"created_at"`
}

// PresignatureRequest represents a request to generate a presignature
// Presignatures are generated offline and used later for fast signing
type PresignatureRequest struct {
	// ID uniquely identifies this presignature generation request
	ID string `json:"id"`
	// Status indicates the current state: "pending", "in_progress", "completed", or "failed"
	Status string `json:"status"`
	// CreatedAt is when this request was created
	CreatedAt time.Time `json:"created_at"`
	// UpdatedAt is when this request was last modified
	UpdatedAt time.Time `json:"updated_at"`
}

// PartialSignatureMessage represents a partial signature from a node
// Used in fast signing to exchange partial signatures computed from presignatures
type PartialSignatureMessage struct {
	// ID uniquely identifies this partial signature message
	ID string `json:"id"`
	// RequestID links this to a specific signing request
	RequestID string `json:"request_id"`
	// FromNode is the identifier of the node that sent this partial signature
	FromNode string `json:"from_node"`
	// Payload contains the partial signature data (JSON-encoded)
	Payload string `json:"payload"`
	// CreatedAt is when this partial signature was posted
	CreatedAt time.Time `json:"created_at"`
}

// Store manages all signing requests, presignature requests, and messages
// It provides thread-safe storage with mutex protection
type Store struct {
	mu sync.RWMutex
	// requests maps request ID to SigningRequest
	requests map[string]*SigningRequest
	// presignatureRequests maps request ID to PresignatureRequest
	presignatureRequests map[string]*PresignatureRequest
	// messages stores all NodeMessages, indexed by message ID
	messages map[string]*NodeMessage
	// messagesByRequest indexes messages by request ID for faster lookup
	messagesByRequest map[string][]*NodeMessage
	// presignatureMessages stores messages for presignature generation
	presignatureMessages map[string]*NodeMessage
	// presignatureMessagesByRequest indexes presignature messages by request ID
	presignatureMessagesByRequest map[string][]*NodeMessage
	// partialSignatures stores partial signature messages for fast signing
	partialSignatures map[string]*PartialSignatureMessage
	// partialSignaturesByRequest indexes partial signatures by request ID
	partialSignaturesByRequest map[string][]*PartialSignatureMessage
	// publicKey stores the shared public key (set by first node after keygen)
	publicKey string
}

// NewStore creates a new empty Store
func NewStore() *Store {
	return &Store{
		requests:                      make(map[string]*SigningRequest),
		presignatureRequests:          make(map[string]*PresignatureRequest),
		messages:                      make(map[string]*NodeMessage),
		messagesByRequest:             make(map[string][]*NodeMessage),
		presignatureMessages:          make(map[string]*NodeMessage),
		presignatureMessagesByRequest: make(map[string][]*NodeMessage),
		partialSignatures:             make(map[string]*PartialSignatureMessage),
		partialSignaturesByRequest:    make(map[string][]*PartialSignatureMessage),
	}
}
