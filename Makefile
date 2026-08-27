.PHONY: build e2e-test smoke-test integration-test

build:
	$(MAKE) -C veritixpay/contract/token build

e2e-test:
	@echo "Running end-to-end event lifecycle test..."
	@bash scripts/e2e_event_lifecycle.sh

# Closes #558: deploy to testnet and verify basic operations.
smoke-test:
	@echo "Running testnet smoke test..."
	@bash scripts/smoke_test.sh

integration-test:
	@echo "Running Soroban CLI integration test..."
	@bash scripts/integration_test.sh

# Closes #567: install pre-commit hooks for cargo fmt and clippy.
install-hooks:
	cp .hooks/pre-commit .git/hooks/pre-commit
	chmod +x .git/hooks/pre-commit
	@echo "Pre-commit hook installed!"
