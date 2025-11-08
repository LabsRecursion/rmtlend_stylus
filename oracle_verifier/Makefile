-include .env
export
deploy_local:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC) --no-verify --private-key $(PRIVATE_KEY)

abi:
	@echo "writting to a file ..."
	@cargo stylus export-abi --json --output oracle_verifier.json

check:
	@echo "checking the contract"
	@cargo stylus check

deploy_testnet:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC_TESTNET) --no-verify --private-key $(PRIVATE_KEY_TEST)

verify_testnet:
	@echo "verifying the contract"
	@cargo stylus verify --deployment-tx $(TX_HASH)

set_addresses:
	@echo "set up addresses for the contract interaction"
	@cast send 0xf5c936a4bb1d67a4a5af74ce9b36922a8f85e967 --rpc-url=$(RPC_TESTNET) --private-key $(PRIVATE_KEY_TEST) "setAddresses(address,address)" 0xb0a7a7c599c08fa374b8cd24041d5e2b0960aacb 0xe469618196246754a97483763ff85707f3996049