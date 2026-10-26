// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title PresensiKTM
/// @notice Buku besar kehadiran dari tap KTM. Hanya hash yang disimpan:
///         tidak ada nama, NIM, atau UID kartu di blockchain.
contract PresensiKTM {
    address public immutable operator;

    /// bukti => timestamp blok saat dicatat (0 = belum tercatat)
    mapping(bytes32 => uint64) public tercatat;
    uint256 public jumlah;

    event Hadir(bytes32 indexed sesi, bytes32 indexed mahasiswa, bytes32 bukti, uint64 waktu);

    error BukanOperator();
    error SudahTercatat();

    constructor() {
        operator = msg.sender;
    }

    /// @param sesi hash sesi kelas
    /// @param mahasiswa hash UID KTM (bergaram, tidak bisa dibalik ke UID)
    /// @param bukti hash catatan kehadiran di server
    function catat(bytes32 sesi, bytes32 mahasiswa, bytes32 bukti) external {
        if (msg.sender != operator) revert BukanOperator();
        if (tercatat[bukti] != 0) revert SudahTercatat();
        uint64 waktu = uint64(block.timestamp);
        tercatat[bukti] = waktu;
        unchecked { jumlah++; }
        emit Hadir(sesi, mahasiswa, bukti, waktu);
    }
}
