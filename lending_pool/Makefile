-include .env
export
deploy_local:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC) --no-verify --private-key $(PRIVATE_KEY)

abi:
	@echo "writting to a file ..."
	@cargo stylus export-abi --json --output lending_pool.json

check:
	@echo "checking the contract"
	@cargo stylus check

deploy_testnet:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC_TESTNET) --no-verify --private-key $(PRIVATE_KEY_TEST) --constructor-args 0xe469618196246754a97483763ff85707f3996049 0x1465423f3a045bd18b0cf6068dec0cb07bfd360d 30

verify_testnet:
	@echo "verifying the contract"
	@cargo stylus verify --deployment-tx $(TX_HASH)