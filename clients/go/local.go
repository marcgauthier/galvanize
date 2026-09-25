package galvanize

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
)

// LocalCLI runs operations that GALVANIZE exposes only through its local binary.
// BinaryPath and ConfigPath are explicit so callers do not depend on PATH or
// the system-wide default configuration.
type LocalCLI struct {
	BinaryPath string
	ConfigPath string
	WorkingDir string
	Stdout     io.Writer
	Stderr     io.Writer
}

// Run invokes a GALVANIZE subcommand with a context and argument vector.
// The process inherits named environment variables, including key variables.
func (c LocalCLI) Run(ctx context.Context, args ...string) error {
	if c.BinaryPath == "" {
		return errors.New("GALVANIZE binary path is required")
	}
	if len(args) == 0 {
		return errors.New("GALVANIZE subcommand is required")
	}
	argv := make([]string, 0, len(args)+2)
	if c.ConfigPath != "" {
		argv = append(argv, "--config", c.ConfigPath)
	}
	argv = append(argv, args...)
	cmd := exec.CommandContext(ctx, c.BinaryPath, argv...)
	cmd.Dir = c.WorkingDir
	cmd.Stdout, cmd.Stderr = c.Stdout, c.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("galvanize %s: %w", args[0], err)
	}
	return nil
}

func (c LocalCLI) RunAgent(ctx context.Context) error { return c.Run(ctx, "agent") }
func (c LocalCLI) Backup(ctx context.Context, destination string) error {
	return c.Run(ctx, "backup", destination)
}
func (c LocalCLI) Restore(ctx context.Context, source string, selfActorID bool, actorID string) error {
	args := []string{"restore", source}
	if selfActorID {
		args = append(args, "--self-actor-id")
	}
	if actorID != "" {
		args = append(args, "--actor-id", actorID)
	}
	return c.Run(ctx, args...)
}

// Rekey is an offline operation. The named key variables must already be set
// in the process environment; no key material is accepted as an argument.
func (c LocalCLI) Rekey(ctx context.Context, path, currentKeyEnv, newKeyEnv, cipher, newCipher string) error {
	if newKeyEnv == "" {
		return errors.New("new key environment variable name is required")
	}
	if _, ok := os.LookupEnv(newKeyEnv); !ok {
		return fmt.Errorf("new key environment variable %q is unset", newKeyEnv)
	}
	args := []string{"rekey", "--new-key-env", newKeyEnv}
	if path != "" {
		args = append(args, "--path", path)
	}
	if currentKeyEnv != "" {
		if _, ok := os.LookupEnv(currentKeyEnv); !ok {
			return fmt.Errorf("current key environment variable %q is unset", currentKeyEnv)
		}
		args = append(args, "--current-key-env", currentKeyEnv)
	}
	if cipher != "" {
		args = append(args, "--cipher", cipher)
	}
	if newCipher != "" {
		args = append(args, "--new-cipher", newCipher)
	}
	return c.Run(ctx, args...)
}

func (c LocalCLI) ConsulSync(ctx context.Context) error { return c.Run(ctx, "consul", "sync") }
func (c LocalCLI) Template(ctx context.Context, once bool, mappings ...string) error {
	if len(mappings) == 0 {
		return errors.New("at least one template mapping is required")
	}
	args := []string{"template"}
	if once {
		args = append(args, "--once")
	}
	return c.Run(ctx, append(args, mappings...)...)
}
func (c LocalCLI) GenerateCA(ctx context.Context) error { return c.Run(ctx, "tls", "ca", "generate") }
func (c LocalCLI) GenerateServerCert(ctx context.Context, ip, caKeyPath, caCertPath string) error {
	return c.Run(ctx, "tls", "server", "generate", ip, "--ca-key", caKeyPath, "--ca-cert", caCertPath)
}
func (c LocalCLI) GenerateClientCert(ctx context.Context, caKeyPath, caCertPath string) error {
	return c.Run(ctx, "tls", "client", "generate", "--ca-key", caKeyPath, "--ca-cert", caCertPath)
}
func (c LocalCLI) SampleChanges(ctx context.Context, destination string, limit uint32) error {
	return c.Run(ctx, "db", "sample-changes", "--out", destination, "--limit", strconv.FormatUint(uint64(limit), 10))
}

// LockDB runs a command while the local database lock is held. The GALVANIZE
// CLI interprets commandText, so supply it only from trusted operator input.
func (c LocalCLI) LockDB(ctx context.Context, commandText string) error {
	if commandText == "" {
		return errors.New("lock command is required")
	}
	return c.Run(ctx, "db", "lock", commandText)
}
