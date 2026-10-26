// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title SertifikatKTM
/// @notice Sertifikat kehadiran acara (seminar, workshop) yang terbit saat peserta tap KTM.
///         Hanya hash yang disimpan: tidak ada nama, NIM, atau UID kartu di blockchain.
contract SertifikatKTM {
    address public immutable operator;

    /// kode sertifikat => timestamp blok saat terbit (0 = tidak pernah terbit)
    mapping(bytes32 => uint64) public terbit;
    uint256 public jumlah;

    event Diterbitkan(bytes32 indexed acara, bytes32 indexed penerima, bytes32 kode, uint64 waktu);

    error BukanOperator();
    error SudahTerbit();

    constructor() {
        operator = msg.sender;
    }

    /// @param acara hash acara
    /// @param penerima hash UID KTM penerima (bergaram, tidak bisa dibalik)
    /// @param kode hash sertifikat yang dibagikan sebagai tautan verifikasi
    function terbitkan(bytes32 acara, bytes32 penerima, bytes32 kode) external {
        if (msg.sender != operator) revert BukanOperator();
        if (terbit[kode] != 0) revert SudahTerbit();
        uint64 waktu = uint64(block.timestamp);
        terbit[kode] = waktu;
        unchecked { jumlah++; }
        emit Diterbitkan(acara, penerima, kode, waktu);
    }
}
