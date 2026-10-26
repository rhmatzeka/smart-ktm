// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {PresensiKTM} from "../src/PresensiKTM.sol";

contract PresensiKTMTest is Test {
    PresensiKTM p;
    bytes32 constant SESI = keccak256("sesi");
    bytes32 constant MHS = keccak256("mhs");
    bytes32 constant BUKTI = keccak256("bukti");

    event Hadir(bytes32 indexed sesi, bytes32 indexed mahasiswa, bytes32 bukti, uint64 waktu);

    function setUp() public {
        p = new PresensiKTM();
    }

    function test_catatMenyimpanWaktuDanEvent() public {
        vm.warp(1_800_000_000);
        vm.expectEmit(true, true, false, true);
        emit Hadir(SESI, MHS, BUKTI, 1_800_000_000);
        p.catat(SESI, MHS, BUKTI);
        assertEq(p.tercatat(BUKTI), 1_800_000_000);
        assertEq(p.jumlah(), 1);
    }

    function test_tidakBisaDicatatDuaKali() public {
        p.catat(SESI, MHS, BUKTI);
        vm.expectRevert(PresensiKTM.SudahTercatat.selector);
        p.catat(SESI, MHS, BUKTI);
    }

    function test_hanyaOperator() public {
        vm.prank(address(0xBEEF));
        vm.expectRevert(PresensiKTM.BukanOperator.selector);
        p.catat(SESI, MHS, BUKTI);
    }
}
