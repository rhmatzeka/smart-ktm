// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {SertifikatKTM} from "../src/SertifikatKTM.sol";

contract SertifikatKTMTest is Test {
    SertifikatKTM s;
    bytes32 constant ACARA = keccak256("seminar");
    bytes32 constant PENERIMA = keccak256("mhs");
    bytes32 constant KODE = keccak256("kode");

    event Diterbitkan(bytes32 indexed acara, bytes32 indexed penerima, bytes32 kode, uint64 waktu);

    function setUp() public {
        s = new SertifikatKTM();
    }

    function test_terbitkanMenyimpanWaktuDanEvent() public {
        vm.warp(1_800_000_000);
        vm.expectEmit(true, true, false, true);
        emit Diterbitkan(ACARA, PENERIMA, KODE, 1_800_000_000);
        s.terbitkan(ACARA, PENERIMA, KODE);
        assertEq(s.terbit(KODE), 1_800_000_000);
        assertEq(s.jumlah(), 1);
    }

    function test_tidakBisaTerbitDuaKali() public {
        s.terbitkan(ACARA, PENERIMA, KODE);
        vm.expectRevert(SertifikatKTM.SudahTerbit.selector);
        s.terbitkan(ACARA, PENERIMA, KODE);
    }

    function test_hanyaOperator() public {
        vm.prank(address(0xBEEF));
        vm.expectRevert(SertifikatKTM.BukanOperator.selector);
        s.terbitkan(ACARA, PENERIMA, KODE);
    }
}
