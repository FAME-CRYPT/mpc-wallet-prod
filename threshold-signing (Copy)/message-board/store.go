package main

import (
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"time"
)

// CreateRequest creates a new signing request with the given message
// Returns the newly created request with a unique ID
func (s *Store) CreateRequest(message string) (*SigningRequest, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Generate a unique request ID
	id, err := generateID()
	if err != nil {
		return nil, err
	}

	// Create the request object
	now := time.Now()
	req := &SigningRequest{
		ID:        id,
		Message:   message,
		Status:    "pending",
		CreatedAt: now,
		UpdatedAt: now,
	}

	// Store it
	s.requests[id] = req

	return req, nil
}

// GetRequest retrieves a signing request by ID
// Returns nil if not found
func (s *Store) GetRequest(id string) *SigningRequest {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return s.requests[id]
}

// ListRequests retrieves all signing requests, optionally filtered by status
// If statusFilter is empty, returns all requests
func (s *Store) ListRequests(statusFilter string) []*SigningRequest {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var result []*SigningRequest
	for _, req := range s.requests {
		// If no filter, include all requests
		// If filter is set, only include matching requests
		if statusFilter == "" || req.Status == statusFilter {
			result = append(result, req)
		}
	}

	return result
}

// UpdateRequestStatus updates the status of a signing request
// Returns error if request not found
func (s *Store) UpdateRequestStatus(id string, status string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	req, exists := s.requests[id]
	if !exists {
		return fmt.Errorf("request not found: %s", id)
	}

	req.Status = status
	req.UpdatedAt = time.Now()

	return nil
}

// SetRequestSignature sets the final signature for a completed request
// Also updates status to "completed"
func (s *Store) SetRequestSignature(id string, signature string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	req, exists := s.requests[id]
	if !exists {
		return fmt.Errorf("request not found: %s", id)
	}

	req.Signature = signature
	req.Status = "completed"
	req.UpdatedAt = time.Now()

	return nil
}

// PostMessage stores a new message from a node
// The message is indexed by both message ID and request ID for efficient lookup
func (s *Store) PostMessage(requestID, fromNode, toNode string, round int, payload string) (*NodeMessage, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Auto-create request if it doesn't exist (for keygen/auxgen protocols)
	if _, exists := s.requests[requestID]; !exists {
		// Create a placeholder request for protocol coordination
		now := time.Now()
		s.requests[requestID] = &SigningRequest{
			ID:        requestID,
			Message:   requestID, // Use request ID as message for protocol requests
			Status:    "pending",
			CreatedAt: now,
			UpdatedAt: now,
		}
	}

	// Generate a unique message ID
	id, err := generateID()
	if err != nil {
		return nil, err
	}

	// Create the message
	msg := &NodeMessage{
		ID:        id,
		RequestID: requestID,
		FromNode:  fromNode,
		ToNode:    toNode,
		Round:     round,
		Payload:   payload,
		CreatedAt: time.Now(),
	}

	// Store in both indexes
	s.messages[id] = msg
	s.messagesByRequest[requestID] = append(s.messagesByRequest[requestID], msg)

	return msg, nil
}

// GetMessagesForRequest retrieves all messages for a specific signing request
// Nodes can optionally filter by recipient (toNode) to get only their messages
func (s *Store) GetMessagesForRequest(requestID string, toNode string) []*NodeMessage {
	s.mu.RLock()
	defer s.mu.RUnlock()

	allMessages := s.messagesByRequest[requestID]
	if toNode == "" {
		// Return all messages for this request
		return allMessages
	}

	// Filter messages for specific node
	// Include messages addressed to this node OR broadcast messages (toNode = "*")
	var filtered []*NodeMessage
	for _, msg := range allMessages {
		if msg.ToNode == toNode || msg.ToNode == "*" {
			filtered = append(filtered, msg)
		}
	}

	return filtered
}

// generateID creates a random hex-encoded ID
func generateID() (string, error) {
	bytes := make([]byte, 16)
	if _, err := rand.Read(bytes); err != nil {
		return "", err
	}
	return hex.EncodeToString(bytes), nil
}

// GetPublicKey returns the shared public key
// Returns empty string if not set yet
func (s *Store) GetPublicKey() string {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.publicKey
}

// SetPublicKey stores the shared public key
// This should be called by a node after keygen completes
func (s *Store) SetPublicKey(publicKey string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.publicKey = publicKey
}

// CreatePresignatureRequest creates a new presignature generation request
// Returns the newly created request with a unique ID
func (s *Store) CreatePresignatureRequest() (*PresignatureRequest, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Generate a unique request ID
	id, err := generateID()
	if err != nil {
		return nil, err
	}

	// Create the request object
	now := time.Now()
	req := &PresignatureRequest{
		ID:        id,
		Status:    "pending",
		CreatedAt: now,
		UpdatedAt: now,
	}

	// Store it
	s.presignatureRequests[id] = req

	return req, nil
}

// GetPresignatureRequest retrieves a presignature request by ID
// Returns nil if not found
func (s *Store) GetPresignatureRequest(id string) *PresignatureRequest {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return s.presignatureRequests[id]
}

// ListPresignatureRequests retrieves all presignature requests, optionally filtered by status
// If statusFilter is empty, returns all requests
func (s *Store) ListPresignatureRequests(statusFilter string) []*PresignatureRequest {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var result []*PresignatureRequest
	for _, req := range s.presignatureRequests {
		if statusFilter == "" || req.Status == statusFilter {
			result = append(result, req)
		}
	}

	return result
}

// UpdatePresignatureRequestStatus updates the status of a presignature request
// Returns error if request not found
func (s *Store) UpdatePresignatureRequestStatus(id string, status string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	req, exists := s.presignatureRequests[id]
	if !exists {
		return fmt.Errorf("presignature request not found: %s", id)
	}

	req.Status = status
	req.UpdatedAt = time.Now()

	return nil
}

// PostPresignatureMessage stores a new message for presignature generation
// The message is indexed by both message ID and request ID for efficient lookup
func (s *Store) PostPresignatureMessage(requestID, fromNode, toNode string, round int, payload string) (*NodeMessage, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Auto-create presignature request if it doesn't exist
	if _, exists := s.presignatureRequests[requestID]; !exists {
		now := time.Now()
		s.presignatureRequests[requestID] = &PresignatureRequest{
			ID:        requestID,
			Status:    "pending",
			CreatedAt: now,
			UpdatedAt: now,
		}
	}

	// Generate a unique message ID
	id, err := generateID()
	if err != nil {
		return nil, err
	}

	// Create the message
	msg := &NodeMessage{
		ID:        id,
		RequestID: requestID,
		FromNode:  fromNode,
		ToNode:    toNode,
		Round:     round,
		Payload:   payload,
		CreatedAt: time.Now(),
	}

	// Store in presignature-specific indexes
	s.presignatureMessages[id] = msg
	s.presignatureMessagesByRequest[requestID] = append(s.presignatureMessagesByRequest[requestID], msg)

	return msg, nil
}

// GetPresignatureMessagesForRequest retrieves all presignature messages for a specific request
// Nodes can optionally filter by recipient (toNode) to get only their messages
func (s *Store) GetPresignatureMessagesForRequest(requestID string, toNode string) []*NodeMessage {
	s.mu.RLock()
	defer s.mu.RUnlock()

	allMessages := s.presignatureMessagesByRequest[requestID]
	if toNode == "" {
		return allMessages
	}

	// Filter messages for specific node
	var filtered []*NodeMessage
	for _, msg := range allMessages {
		if msg.ToNode == toNode || msg.ToNode == "*" {
			filtered = append(filtered, msg)
		}
	}

	return filtered
}

// PostPartialSignature posts a partial signature for a signing request
// Returns the created PartialSignatureMessage
func (s *Store) PostPartialSignature(requestID, fromNode, payload string) (*PartialSignatureMessage, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Generate a unique ID for this partial signature
	id, err := generateID()
	if err != nil {
		return nil, err
	}

	// Create the partial signature message
	partialSig := &PartialSignatureMessage{
		ID:        id,
		RequestID: requestID,
		FromNode:  fromNode,
		Payload:   payload,
		CreatedAt: time.Now(),
	}

	// Store it
	s.partialSignatures[id] = partialSig
	s.partialSignaturesByRequest[requestID] = append(s.partialSignaturesByRequest[requestID], partialSig)

	return partialSig, nil
}

// GetPartialSignaturesForRequest retrieves all partial signatures for a signing request
func (s *Store) GetPartialSignaturesForRequest(requestID string) []*PartialSignatureMessage {
	s.mu.RLock()
	defer s.mu.RUnlock()

	return s.partialSignaturesByRequest[requestID]
}
