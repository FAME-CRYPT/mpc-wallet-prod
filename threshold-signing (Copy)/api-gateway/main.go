package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
)

// messageBoardURL is the base URL for the MessageBoard service
// It is configured via environment variable at startup
var messageBoardURL string

func main() {
	// Configure MessageBoard URL from environment
	messageBoardURL = os.Getenv("MESSAGE_BOARD_URL")
	if messageBoardURL == "" {
		messageBoardURL = "http://message-board:8080"
	}

	// Get port from environment variable, default to 8000
	port := os.Getenv("PORT")
	if port == "" {
		port = "8000"
	}

	// Register HTTP handlers
	http.HandleFunc("/health", handleHealth)
	http.HandleFunc("/publickey", handlePublicKey)
	http.HandleFunc("/sign", handleSignRequest)
	http.HandleFunc("/status/", handleStatusRequest)

	// Start the HTTP server
	log.Printf("API Gateway starting on port %s", port)
	log.Fatal(http.ListenAndServe(":"+port, nil))
}

// handleHealth responds to health check requests
// This is used by container orchestration to verify the service is running
func handleHealth(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
	w.Write([]byte("OK"))
}

// handlePublicKey returns the shared public key from the MessageBoard
// This public key can be used to verify any signatures produced by the threshold signing system
func handlePublicKey(w http.ResponseWriter, r *http.Request) {
	// Only accept GET requests
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Query the MessageBoard for the public key
	publicKey, err := queryPublicKey()
	if err != nil {
		log.Printf("Error querying public key: %v", err)
		http.Error(w, "Failed to get public key", http.StatusInternalServerError)
		return
	}

	// Return the public key
	response := PublicKeyResponse{
		PublicKey: publicKey,
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

// queryPublicKey retrieves the shared public key from the MessageBoard
func queryPublicKey() (string, error) {
	resp, err := http.Get(messageBoardURL + "/publickey")
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("message board returned status %d: %s", resp.StatusCode, string(body))
	}

	var result map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return "", err
	}

	publicKey, ok := result["public_key"].(string)
	if !ok {
		return "", fmt.Errorf("invalid response from message board")
	}

	return publicKey, nil
}

// handleSignRequest processes incoming signing requests
// It validates the request, forwards it to the MessageBoard, and returns a request ID
func handleSignRequest(w http.ResponseWriter, r *http.Request) {
	// Only accept POST requests
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Parse the JSON request body
	var req SignRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "Invalid request body", http.StatusBadRequest)
		return
	}

	// Validate that message is not empty
	if req.Message == "" {
		http.Error(w, "Message is required", http.StatusBadRequest)
		return
	}

	// Forward the signing request to the MessageBoard
	// The MessageBoard will coordinate the threshold signing protocol
	requestID, err := forwardToMessageBoard(req.Message)
	if err != nil {
		log.Printf("Error forwarding to message board: %v", err)
		http.Error(w, "Failed to process signing request", http.StatusInternalServerError)
		return
	}

	// Return the request ID to the client
	// They can use this to check the status later
	response := SignResponse{
		RequestID: requestID,
		Status:    "pending",
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

// forwardToMessageBoard sends a signing request to the MessageBoard service
// Returns the request ID assigned by the MessageBoard
func forwardToMessageBoard(message string) (string, error) {
	// Prepare the request payload
	data := map[string]string{
		"message": message,
	}
	jsonData, err := json.Marshal(data)
	if err != nil {
		return "", err
	}

	// Send HTTP POST to MessageBoard
	resp, err := http.Post(
		messageBoardURL+"/requests",
		"application/json",
		bytes.NewBuffer(jsonData),
	)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	// Check for errors from MessageBoard
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusCreated {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("message board returned status %d: %s", resp.StatusCode, string(body))
	}

	// Extract the request ID from the response
	var result map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return "", err
	}

	requestID, ok := result["request_id"].(string)
	if !ok {
		return "", fmt.Errorf("invalid response from message board")
	}

	return requestID, nil
}

// handleStatusRequest checks the status of a signing request
// URL format: /status/{request_id}
func handleStatusRequest(w http.ResponseWriter, r *http.Request) {
	// Only accept GET requests
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Extract request ID from URL path
	// Path is like "/status/abc123", we want "abc123"
	requestID := r.URL.Path[len("/status/"):]
	if requestID == "" {
		http.Error(w, "Request ID is required", http.StatusBadRequest)
		return
	}

	// Query the MessageBoard for the current status
	status, signature, err := queryMessageBoard(requestID)
	if err != nil {
		log.Printf("Error querying message board: %v", err)
		http.Error(w, "Failed to get status", http.StatusInternalServerError)
		return
	}

	// Build and return the response
	response := SignatureStatusResponse{
		RequestID: requestID,
		Status:    status,
		Signature: signature,
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

// queryMessageBoard retrieves the status and signature (if ready) for a request
// Returns status string, signature string (empty if not ready), and error
func queryMessageBoard(requestID string) (string, string, error) {
	// Send GET request to MessageBoard
	resp, err := http.Get(messageBoardURL + "/requests/" + requestID)
	if err != nil {
		return "", "", err
	}
	defer resp.Body.Close()

	// Handle not found case
	if resp.StatusCode == http.StatusNotFound {
		return "not_found", "", nil
	}

	// Check for other errors
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return "", "", fmt.Errorf("message board returned status %d: %s", resp.StatusCode, string(body))
	}

	// Parse the response from MessageBoard
	var result map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return "", "", err
	}

	// Extract status and signature (signature may be empty)
	status, _ := result["status"].(string)
	signature, _ := result["signature"].(string)

	return status, signature, nil
}
