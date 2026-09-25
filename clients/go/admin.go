package galvanize

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
)

// AdminCommand is the JSON representation of one existing GALVANIZE admin
// socket command. Use the constructors below to create supported commands.
type AdminCommand json.RawMessage

// AdminResponse is one command log, data, success, or error event.
type AdminResponse struct {
	Kind      string          `json:"-"`
	Level     string          `json:"-"`
	Message   string          `json:"-"`
	Timestamp string          `json:"-"`
	Data      json.RawMessage `json:"-"`
	Error     string          `json:"-"`
}

func (r *AdminResponse) UnmarshalJSON(data []byte) error {
	if len(data) > 0 && data[0] == '"' {
		return json.Unmarshal(data, &r.Kind)
	}
	var event map[string]json.RawMessage
	if err := json.Unmarshal(data, &event); err != nil {
		return err
	}
	if len(event) != 1 {
		return errors.New("invalid admin response event")
	}
	for name, raw := range event {
		r.Kind = name
		switch name {
		case "Log":
			var value struct {
				Level     string `json:"level"`
				Message   string `json:"msg"`
				Timestamp string `json:"ts"`
			}
			if err := json.Unmarshal(raw, &value); err != nil {
				return err
			}
			r.Level, r.Message, r.Timestamp = value.Level, value.Message, value.Timestamp
		case "Json":
			r.Data = append(r.Data[:0], raw...)
		case "Error":
			var value struct {
				Message string `json:"msg"`
			}
			if err := json.Unmarshal(raw, &value); err != nil {
				return err
			}
			r.Error = value.Message
		case "Success":
		default:
			return fmt.Errorf("unknown admin response %q", name)
		}
	}
	return nil
}

// AdminResult contains the ordered events emitted by the admin command.
type AdminResult struct {
	Responses []AdminResponse `json:"responses"`
}

// AdminCommandError reports a command-level failure while retaining its logs.
type AdminCommandError struct {
	Result  AdminResult
	Message string
}

func (e *AdminCommandError) Error() string { return "GALVANIZE admin command failed: " + e.Message }

// RunAdminCommand executes one command on the dedicated mTLS control listener.
func (c *Client) RunAdminCommand(ctx context.Context, command AdminCommand) (AdminResult, error) {
	var result AdminResult
	if len(command) == 0 || !json.Valid(command) {
		return result, errors.New("invalid admin command JSON")
	}
	err := c.jsonRequest(ctx, http.MethodPost, "/v1/admin/commands", nil, json.RawMessage(command), &result, c.adminURL)
	if err != nil {
		return result, err
	}
	for _, response := range result.Responses {
		if response.Kind == "Error" {
			return result, &AdminCommandError{Result: result, Message: response.Error}
		}
	}
	return result, nil
}

func adminUnit(name string) AdminCommand {
	encoded, _ := json.Marshal(name)
	return AdminCommand(encoded)
}

func adminNested(group, variant string, args any) AdminCommand {
	var inner any = variant
	if args != nil {
		inner = map[string]any{variant: args}
	}
	encoded, _ := json.Marshal(map[string]any{group: inner})
	return AdminCommand(encoded)
}

func AdminPing() AdminCommand               { return adminUnit("Ping") }
func AdminReloadSchema() AdminCommand       { return adminUnit("Reload") }
func AdminReloadDictionaries() AdminCommand { return adminUnit("ReloadDicts") }
func AdminSyncGenerate() AdminCommand       { return adminNested("Sync", "Generate", nil) }
func AdminSyncReconcileGaps() AdminCommand  { return adminNested("Sync", "ReconcileGaps", nil) }
func AdminSyncCheckBookieConsistency() AdminCommand {
	return adminNested("Sync", "CheckBookieConsistency", nil)
}

func AdminSyncProcessBufferedChanges(actorID string, version, chunkSize uint64) AdminCommand {
	return adminNested("Sync", "ProcessBufferedChanges", map[string]any{
		"actor_id": actorID, "version": version, "chunk_size": chunkSize,
	})
}

func AdminClusterRejoin() AdminCommand  { return adminNested("Cluster", "Rejoin", nil) }
func AdminClusterMembers() AdminCommand { return adminNested("Cluster", "Members", nil) }
func AdminClusterMembershipStates() AdminCommand {
	return adminNested("Cluster", "MembershipStates", nil)
}
func AdminClusterSetID(clusterID uint16) AdminCommand {
	return adminNested("Cluster", "SetId", clusterID)
}

func AdminActorVersion(actorID string, version uint64) AdminCommand {
	return adminNested("Actor", "Version", map[string]any{"actor_id": actorID, "version": version})
}

func AdminSubscriptionsList() AdminCommand { return adminNested("Subs", "List", nil) }
func AdminSubscriptionInfo(hash, id *string) AdminCommand {
	return adminNested("Subs", "Info", map[string]any{"hash": hash, "id": id})
}
func AdminSetLogFilter(filter string) AdminCommand {
	return adminNested("Log", "Set", map[string]any{"filter": filter})
}
func AdminResetLogFilter() AdminCommand { return adminNested("Log", "Reset", nil) }
func AdminPlumtreeStats() AdminCommand  { return adminNested("Plumtree", "Stats", nil) }
