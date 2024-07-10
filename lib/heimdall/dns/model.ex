defmodule Heimdall.DNS.Model do
  def parse_labels(labels, data) do
    <<label_len::8, data::bitstring>> = data

    case label_len do
      0 ->
        [Enum.reverse(labels), data]

      _ ->
        <<label::binary-size(label_len), data::bitstring>> = data
        parse_labels([label | labels], data)
    end
  end

  def qr(n) do
    case n do
      0 -> :request
      1 -> :response
      _ -> throw("Invalid QR value #{n}")
    end
  end

  def opcode(n) do
    case n do
      0 -> :query
      1 -> :iquery
      2 -> :status
      _ -> throw("Invalid opcode value #{n}")
    end
  end

  def qtype(n) do
    case n do
      1 -> :a
      2 -> :ns
      5 -> :cname
      6 -> :soa
      12 -> :ptr
      13 -> :hinfo
      15 -> :mx
      16 -> :text
      17 -> :rp
      18 -> :afsdb
      24 -> :sig
      25 -> :key
      28 -> :aaaa
      29 -> :loc
      33 -> :srv
      35 -> :naptr
      36 -> :kx
      37 -> :cert
      39 -> :dname
      41 -> :opt
      42 -> :apl
      43 -> :ds
      44 -> :sshfp
      45 -> :ipseckey
      46 -> :rrsig
      47 -> :nsec
      48 -> :dnskey
      49 -> :dhcid
      50 -> :nsec3
      51 -> :nsec3param
      52 -> :tlsa
      53 -> :smimea
      55 -> :hip
      59 -> :cds
      60 -> :cdnskey
      61 -> :openpgpkey
      62 -> :csync
      63 -> :zonemd
      64 -> :svcb
      65 -> :https
      108 -> :eui48
      109 -> :eui64
      249 -> :tkey
      250 -> :tsig
      252 -> :axfr
      253 -> :mailb
      254 -> :maila
      255 -> :all
      256 -> :uri
      257 -> :caa
      32768 -> :ta
      32769 -> :dlv
      _ -> throw("Unknown QTYPE value #{n}")
    end
  end

  def qclass(n) do
    case n do
      1 -> :in
      2 -> :cs
      3 -> :ch
      4 -> :hesiod
      _ -> throw("Unknown QCLASS value #{n}")
    end
  end
end
