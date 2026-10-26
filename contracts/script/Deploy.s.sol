// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {PresensiKTM} from "../src/PresensiKTM.sol";

contract Deploy is Script {
    function run() external {
        vm.startBroadcast();
        PresensiKTM p = new PresensiKTM();
        vm.stopBroadcast();
        console.log("PresensiKTM:", address(p));
    }
}
