// Command audit-witness is the chaincode binary registered with the
// Fabric peer. It wires the AuditWitnessContract into the contract-api
// runtime; the peer process invokes it via the chaincode-as-a-service
// or external-builder pattern (per Fabric ops conventions).
package main

import (
	"log"

	"github.com/hyperledger/fabric-contract-api-go/contractapi"

	auditwitness "github.com/recor/chaincode/audit-witness/lib"
)

func main() {
	contract := new(auditwitness.AuditWitnessContract)
	cc, err := contractapi.NewChaincode(contract)
	if err != nil {
		log.Panicf("create audit-witness chaincode: %v", err)
	}
	if err := cc.Start(); err != nil {
		log.Panicf("start audit-witness chaincode: %v", err)
	}
}
