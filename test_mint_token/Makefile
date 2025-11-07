-include .env
export
# 0x4a2ba922052ba54e29c5417bc979daaf7d5fe4f4
deploy_local:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC) --no-verify --private-key $(PRIVATE_KEY)

abi:
	@echo "writting to a file ..."
	@cargo stylus export-abi --json --output mock_erc20.json

check:
	@echo "checking the contract"
	@cargo stylus check

deploy_testnet:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC_TESTNET) --no-verify --private-key $(PRIVATE_KEY_TEST)

verify_testnet:
	@echo "verifying the contract"
	@cargo stylus verify --deployment-tx $(TX_HASH)