package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"strings"
)

// Global store instance shared across all HTTP handlers
var store *Store

func main() {
	// Initialize the store
	store = NewStore()

	// Get port from environment variable, default to 8080
	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	// Register HTTP handlers
	http.HandleFunc("/health", handleHealth)
	http.HandleFunc("/publickey", handlePublicKey)
	http.HandleFunc("/requests", handleRequests)
	http.HandleFunc("/requests/", handleRequestByID)
	http.HandleFunc("/messages", handleMessages)
	http.HandleFunc("/presignature-requests", handlePresignatureRequests)
	http.HandleFunc("/presignature-requests/", handlePresignatureRequestByID)
	http.HandleFunc("/presignature-messages", handlePresignatureMessages)
	http.HandleFunc("/partial-signatures", handlePartialSignatures)

	// Start the HTTP server
	log.Printf("MessageBoard starting on port %s", port)
	log.Fatal(http.ListenAndServe(":"+port, nil))
}

// handleHealth responds to health check requests
// Used by container orchestration to verify the service is running
func handleHealth(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
	w.Write([]byte("OK"))
}

// handlePresignatureRequests manages presignature request creation and listing
// POST /presignature-requests - creates a new presignature generation request
// GET /presignature-requests?status=pending - lists presignature requests
func handlePresignatureRequests(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		handleCreatePresignatureRequest(w, r)
	case http.MethodGet:
		handleListPresignatureRequests(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleCreatePresignatureRequest creates a new presignature generation request
func handleCreatePresignatureRequest(w http.ResponseWriter, r *http.Request) {
	// Create the presignature request
	req, err := store.CreatePresignatureRequest()
	if err != nil {
		log.Printf("Error creating presignature request: %v", err)
		http.Error(w, "Failed to create presignature request", http.StatusInternalServerError)
		return
	}

	// Return the created request
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{
		"request_id": req.ID,
		"status":     req.Status,
	})
}

// handleListPresignatureRequests lists all presignature requests, optionally filtered by status
func handleListPresignatureRequests(w http.ResponseWriter, r *http.Request) {
	statusFilter := r.URL.Query().Get("status")
	requests := store.ListPresignatureRequests(statusFilter)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"requests": requests,
		"count":    len(requests),
	})
}

// handlePresignatureRequestByID manages individual presignature requests
// GET /presignature-requests/{id} - retrieves presignature request status
// PUT /presignature-requests/{id} - updates presignature request status
func handlePresignatureRequestByID(w http.ResponseWriter, r *http.Request) {
	path := strings.TrimPrefix(r.URL.Path, "/presignature-requests/")
	requestID := strings.Split(path, "/")[0]

	if requestID == "" {
		http.Error(w, "Request ID is required", http.StatusBadRequest)
		return
	}

	switch r.Method {
	case http.MethodGet:
		handleGetPresignatureRequest(w, requestID)
	case http.MethodPut:
		handleUpdatePresignatureRequest(w, r, requestID)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleGetPresignatureRequest retrieves a presignature request by ID
func handleGetPresignatureRequest(w http.ResponseWriter, requestID string) {
	req := store.GetPresignatureRequest(requestID)
	if req == nil {
		http.Error(w, "Presignature request not found", http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(req)
}

// handleUpdatePresignatureRequest updates a presignature request status
func handleUpdatePresignatureRequest(w http.ResponseWriter, r *http.Request, requestID string) {
	var body map[string]string
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	status, ok := body["status"]
	if !ok || status == "" {
		http.Error(w, "Status is required", http.StatusBadRequest)
		return
	}

	if err := store.UpdatePresignatureRequestStatus(requestID, status); err != nil {
		log.Printf("Error updating presignature status: %v", err)
		http.Error(w, "Failed to update presignature request", http.StatusInternalServerError)
		return
	}

	w.WriteHeader(http.StatusOK)
	w.Write([]byte("OK"))
}

// handlePresignatureMessages manages messages for presignature generation
// POST /presignature-messages - node posts a presignature message
// GET /presignature-messages?request_id=X&to_node=Y - node retrieves presignature messages
func handlePresignatureMessages(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		handlePostPresignatureMessage(w, r)
	case http.MethodGet:
		handleGetPresignatureMessages(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handlePostPresignatureMessage allows a node to post a presignature message
func handlePostPresignatureMessage(w http.ResponseWriter, r *http.Request) {
	var body struct {
		RequestID string `json:"request_id"`
		FromNode  string `json:"from_node"`
		ToNode    string `json:"to_node"`
		Round     int    `json:"round"`
		Payload   string `json:"payload"`
	}

	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	if body.RequestID == "" || body.FromNode == "" || body.ToNode == "" || body.Payload == "" {
		http.Error(w, "Missing required fields", http.StatusBadRequest)
		return
	}

	msg, err := store.PostPresignatureMessage(body.RequestID, body.FromNode, body.ToNode, body.Round, body.Payload)
	if err != nil {
		log.Printf("Error posting presignature message: %v", err)
		http.Error(w, "Failed to post presignature message", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{
		"message_id": msg.ID,
	})
}

// handleGetPresignatureMessages retrieves presignature messages for a node
func handleGetPresignatureMessages(w http.ResponseWriter, r *http.Request) {
	requestID := r.URL.Query().Get("request_id")
	if requestID == "" {
		http.Error(w, "request_id parameter is required", http.StatusBadRequest)
		return
	}

	toNode := r.URL.Query().Get("to_node")
	messages := store.GetPresignatureMessagesForRequest(requestID, toNode)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"messages": messages,
	})
}

// handlePublicKey manages the shared public key
// GET /publickey - retrieve the shared public key for signature verification
// POST /publickey - register the public key (called by nodes after keygen)
func handlePublicKey(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		publicKey := store.GetPublicKey()
		if publicKey == "" {
			http.Error(w, "Public key not available yet", http.StatusNotFound)
			return
		}

		response := map[string]string{
			"public_key": publicKey,
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)

	case http.MethodPost:
		var req struct {
			PublicKey string `json:"public_key"`
		}

		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, "Invalid request body", http.StatusBadRequest)
			return
		}

		if req.PublicKey == "" {
			http.Error(w, "Public key is required", http.StatusBadRequest)
			return
		}

		// Only set if not already set, or if same value (idempotent)
		existingKey := store.GetPublicKey()
		if existingKey != "" && existingKey != req.PublicKey {
			http.Error(w, "Public key already set with different value", http.StatusConflict)
			return
		}

		store.SetPublicKey(req.PublicKey)
		w.WriteHeader(http.StatusOK)
		w.Write([]byte("OK"))

	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleMessages manages messages between nodes
// POST /messages - node posts a message to the board
// GET /messages?request_id=X&to_node=Y - node retrieves messages
func handleMessages(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		handlePostMessage(w, r)
	case http.MethodGet:
		handleGetMessages(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handlePostMessage allows a node to post a message to the board
func handlePostMessage(w http.ResponseWriter, r *http.Request) {
	var body struct {
		RequestID string `json:"request_id"`
		FromNode  string `json:"from_node"`
		ToNode    string `json:"to_node"`
		Round     int    `json:"round"`
		Payload   string `json:"payload"`
	}

	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	// Validate required fields
	if body.RequestID == "" || body.FromNode == "" || body.ToNode == "" || body.Payload == "" {
		http.Error(w, "Missing required fields", http.StatusBadRequest)
		return
	}

	// Store the message
	msg, err := store.PostMessage(body.RequestID, body.FromNode, body.ToNode, body.Round, body.Payload)
	if err != nil {
		log.Printf("Error posting message: %v", err)
		http.Error(w, "Failed to post message", http.StatusInternalServerError)
		return
	}

	// Return the created message
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{
		"message_id": msg.ID,
	})
}

// handleGetMessages retrieves messages for a node
// Query parameters: request_id (required), to_node (optional filter)
func handleGetMessages(w http.ResponseWriter, r *http.Request) {
	// Parse query parameters
	requestID := r.URL.Query().Get("request_id")
	if requestID == "" {
		http.Error(w, "request_id parameter is required", http.StatusBadRequest)
		return
	}

	toNode := r.URL.Query().Get("to_node")

	// Retrieve messages from store
	messages := store.GetMessagesForRequest(requestID, toNode)

	// Return the messages
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"messages": messages,
	})
}

// handleRequests manages signing request creation and listing
// POST /requests - creates a new signing request
// GET /requests?status=pending - lists requests (optionally filtered by status)
func handleRequests(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		handleCreateRequest(w, r)
	case http.MethodGet:
		handleListRequests(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleCreateRequest creates a new signing request
func handleCreateRequest(w http.ResponseWriter, r *http.Request) {

	// Parse the incoming request from API Gateway
	var body map[string]string
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	message, ok := body["message"]
	if !ok || message == "" {
		http.Error(w, "Message is required", http.StatusBadRequest)
		return
	}

	// Create the signing request
	req, err := store.CreateRequest(message)
	if err != nil {
		log.Printf("Error creating request: %v", err)
		http.Error(w, "Failed to create request", http.StatusInternalServerError)
		return
	}

	// Return the created request
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]string{
		"request_id": req.ID,
		"status":     req.Status,
	})
}

// handleListRequests lists all signing requests, optionally filtered by status
// GET /requests - lists all requests
// GET /requests?status=pending - lists only pending requests
func handleListRequests(w http.ResponseWriter, r *http.Request) {
	// Get optional status filter from query parameter
	statusFilter := r.URL.Query().Get("status")

	// Get all requests from store
	requests := store.ListRequests(statusFilter)

	// Return the list
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"requests": requests,
		"count":    len(requests),
	})
}

// handleRequestByID manages individual signing requests
// GET /requests/{id} - retrieves request status and signature
// PUT /requests/{id} - updates request (used by nodes to set signature)
func handleRequestByID(w http.ResponseWriter, r *http.Request) {
	// Extract request ID from URL path
	// Path is "/requests/{id}", we want the {id} part
	path := strings.TrimPrefix(r.URL.Path, "/requests/")
	requestID := strings.Split(path, "/")[0]

	if requestID == "" {
		http.Error(w, "Request ID is required", http.StatusBadRequest)
		return
	}

	switch r.Method {
	case http.MethodGet:
		handleGetRequest(w, requestID)
	case http.MethodPut:
		handleUpdateRequest(w, r, requestID)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleGetRequest retrieves a signing request by ID
func handleGetRequest(w http.ResponseWriter, requestID string) {
	req := store.GetRequest(requestID)
	if req == nil {
		http.Error(w, "Request not found", http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(req)
}

// handleUpdateRequest updates a signing request
// Nodes use this to set the final signature or update status
func handleUpdateRequest(w http.ResponseWriter, r *http.Request, requestID string) {
	var body map[string]string
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	// If signature is provided, set it and mark as completed
	if signature, ok := body["signature"]; ok && signature != "" {
		if err := store.SetRequestSignature(requestID, signature); err != nil {
			log.Printf("Error setting signature: %v", err)
			http.Error(w, "Failed to update request", http.StatusInternalServerError)
			return
		}
	} else if status, ok := body["status"]; ok && status != "" {
		// Otherwise update just the status
		if err := store.UpdateRequestStatus(requestID, status); err != nil {
			log.Printf("Error updating status: %v", err)
			http.Error(w, "Failed to update request", http.StatusInternalServerError)
			return
		}
	} else {
		http.Error(w, "Either signature or status is required", http.StatusBadRequest)
		return
	}

	w.WriteHeader(http.StatusOK)
	w.Write([]byte("OK"))
}

// handlePartialSignatures handles partial signature operations for fast signing
func handlePartialSignatures(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		handlePostPartialSignature(w, r)
	case http.MethodGet:
		handleGetPartialSignatures(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handlePostPartialSignature posts a partial signature for a request
func handlePostPartialSignature(w http.ResponseWriter, r *http.Request) {
	var req struct {
		RequestID string `json:"request_id"`
		FromNode  string `json:"from_node"`
		Payload   string `json:"payload"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid JSON", http.StatusBadRequest)
		return
	}

	partialSig, err := store.PostPartialSignature(req.RequestID, req.FromNode, req.Payload)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(partialSig)
}

// handleGetPartialSignatures retrieves partial signatures for a request
func handleGetPartialSignatures(w http.ResponseWriter, r *http.Request) {
	requestID := r.URL.Query().Get("request_id")
	if requestID == "" {
		http.Error(w, "request_id parameter required", http.StatusBadRequest)
		return
	}

	partialSigs := store.GetPartialSignaturesForRequest(requestID)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"partial_signatures": partialSigs,
	})
}
