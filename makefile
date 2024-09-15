SCRIPTS_DIR := scripts
SHELL_SCRIPTS := $(wildcard $(SCRIPTS_DIR)/*.sh)

.PHONY: gen-all
gen-all:
	@echo "Running all shell scripts in $(SCRIPTS_DIR)..."
	@for script in $(SHELL_SCRIPTS); do \
		echo "Executing $$script..."; \
		bash $$script; \
	done
	@echo "All scripts executed."
