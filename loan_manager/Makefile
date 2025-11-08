-include .env
export
deploy_local:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC) --no-verify --private-key $(PRIVATE_KEY)

abi:
	@echo "writting to a file ..."
	@cargo stylus export-abi --json --output loan_manager.json

check:
	@echo "checking the contract"
	@cargo stylus check

deploy_testnet:
	@echo "deploying locally!"
	@cargo stylus deploy -e $(RPC_TESTNET) --no-verify --private-key $(PRIVATE_KEY_TEST) --constructor-args 0x1465423f3a045bd18b0cf6068dec0cb07bfd360d

verify_testnet:
	@echo "verifying the contract"
	@cargo stylus verify -e $(RPC_TESTNET) --deployment-tx $(TX_HASH)

set_addresses:
	@echo "set up addresses for the contract interaction"
	@cast send 0x70642cc837dfb8d345f38e21bb3f06f2a5a4f9fa \
	"setupAddresses(address,address,address)" \
	0x79033c8bbc044cf489b695f64feb304699bc24d6 \
	0x772aaa96488df85ed0e2777eafa90c4a1eb67cc4 \
	0x0958f5d03aef078a918f6f163adc68866ddc9236 \
	--rpc-url $(RPC_TESTNET) \
	--private-key $(PRIVATE_KEY_TEST)
