package main

// SignRequest represents an incoming request to sign a message
// The API Gateway receives this from external clients
type SignRequest struct {
	// Message is the data to be signed (hex-encoded or plain text)
	Message string `json:"message"`
}

// SignResponse is returned immediately after receiving a sign request
// It provides a request ID that can be used to check the status later
type SignResponse struct {
	// RequestID uniquely identifies this signing request
	RequestID string `json:"request_id"`
	// Status indicates the current state: "pending", "in_progress", "completed", or "failed"
	Status string `json:"status"`
}

// SignatureStatusResponse provides the current status of a signing request
// If the signature is ready, it includes the signature data
type SignatureStatusResponse struct {
	// RequestID is the unique identifier for this request
	RequestID string `json:"request_id"`
	// Status indicates the current state: "pending", "in_progress", "completed", or "failed"
	Status string `json:"status"`
	// Signature contains the threshold signature (only present when Status is "completed")
	Signature string `json:"signature,omitempty"`
}

// PublicKeyResponse contains the shared public key for the threshold signing system
// This key can be used to verify any signatures produced by the system
type PublicKeyResponse struct {
	// PublicKey is the shared ECDSA public key (secp256k1) in compressed hex format
	PublicKey string `json:"public_key"`
}
