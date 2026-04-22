.PHONY: help fmt fmt-check check build test lint clippy fix ci up kani-risk kani-strategy kani-execution kani-runtime-safety kani-runtime-kernel kani-data kani-ledger kani-safety-core kani-all

CARGO ?= cargo
COMPOSE ?= docker compose

help:
	@printf "OpenTicker Make targets:\n"
	@printf "  make fmt        Format all Rust code\n"
	@printf "  make fmt-check  Check formatting without changing files\n"
	@printf "  make check      Type-check all workspace crates\n"
	@printf "  make build      Build all workspace crates\n"
	@printf "  make test       Run all workspace tests\n"
	@printf "  make lint       Run Clippy via cargo alias\n"
	@printf "  make clippy     Run Clippy directly\n"
	@printf "  make fix        Apply Clippy autofixes where possible\n"
	@printf "  make ci         Run CI-like checks (fmt-check, lint, test)\n"
	@printf "  make kani-risk  Run Kani proofs for openticker-risk\n"
	@printf "  make kani-strategy  Run Kani proofs for openticker-strategy\n"
	@printf "  make kani-execution  Run Kani proofs for openticker-execution\n"
	@printf "  make kani-runtime-safety  Run Kani proofs for runtime sizing helpers\n"
	@printf "  make kani-runtime-kernel  Run Kani proofs for runtime decision kernel\n"
	@printf "  make kani-data  Run Kani proofs for openticker-data\n"
	@printf "  make kani-ledger  Run Kani proofs for openticker-ledger\n"
	@printf "  make kani-safety-core  Run the initial Kani pilot set\n"
	@printf "  make kani-all   Alias for the current pilot Kani set\n"
	@printf "  make up         Build and start Docker services\n"

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

check:
	$(CARGO) check --workspace

build:
	$(CARGO) build --workspace

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) lint

clippy:
	$(CARGO) clippy --workspace --all-targets --all-features

fix:
	$(CARGO) clippy --fix --workspace --all-targets --all-features --allow-dirty --allow-staged

ci: fmt-check lint test

kani-risk:
	$(CARGO) kani -p openticker-risk

kani-strategy:
	$(CARGO) kani -p openticker-strategy --harness proof_single_indicator_confirmed_required_suppresses_preview --harness proof_single_indicator_sell_without_position_is_noop --harness proof_single_indicator_stays_long_only

kani-execution:
	$(CARGO) kani -p openticker-execution

kani-runtime-safety:
	$(CARGO) kani -p openticker-runtime --harness proof_resolve_order_quantity_never_returns_non_negative_finite_quantity --harness proof_entry_constraints_zero_out_invalid_entries --harness proof_close_constraints_bypass_min_notional_only --harness proof_position_transition_cannot_create_position_from_close_or_noop

kani-runtime-kernel:
	$(CARGO) kani -p openticker-runtime --harness proof_kernel_reject_keeps_position_state --harness proof_kernel_open_allow_sets_next_position_true --harness proof_kernel_open_dust_becomes_noop

kani-data:
	$(CARGO) kani -p openticker-data --harness proof_same_bucket_only_emits_preview --harness proof_bucket_rollover_emits_confirmed_then_preview --harness proof_flush_confirmed_clears_state --harness proof_invalid_trade_inputs_reject

kani-ledger:
	$(CARGO) kani -p openticker-ledger --harness proof_inventory_sell_cannot_go_negative --harness proof_inventory_oversell_always_errors --harness proof_reservation_never_makes_tradeable_room_negative --harness proof_release_cannot_make_committed_notional_negative --harness proof_reconcile_open_fill_keeps_totals_non_negative

kani-safety-core: kani-risk kani-strategy kani-execution kani-runtime-safety

kani-all: kani-safety-core

up:
	$(COMPOSE) up --build -d
