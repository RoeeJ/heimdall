defmodule Heimdall.DNS.Decoder do
  alias Heimdall.DNS.Model

  @spec packet(data :: bitstring()) :: Model.Packet.t()
  def packet(data) when is_bitstring(data) do

    <<id::16, qr::1, opcode::4, aa::1, tc::1, rd::1, ra::1, z::3, rcode::4, data::bitstring>> =
      data

    <<qdcount::16, ancount::16, nscount::16, arcount::16, data::bitstring>> = data

    [questions, data] = Model.Question.parse([], data, qdcount)
    [answers, data] = Model.Answer.parse([], data, ancount)
    [nameservers, data] = Model.Nameserver.parse([], data, nscount)
    [additional, data] = Model.Additional.parse([], data, arcount)

    %Model.Packet{
      id: id,
      qr: qr(qr),
      opcode: opcode(opcode),
      aa: aa,
      tc: tc,
      rd: rd,
      ra: ra,
      z: z,
      rcode: rcode,
      qdcount: qdcount,
      ancount: ancount,
      nscount: nscount,
      arcount: arcount,
      questions: questions,
      answers: answers,
      nameservers: nameservers,
      additional: additional
    }
  end

  @spec labels(labels :: [String.t()], data :: bitstring()) :: {[String.t()], bitstring()}
  def labels(labels, data) do
    <<label_len::8, data::bitstring>> = data

    case label_len do
      0 ->
        [Enum.reverse(labels), data]

      _ ->
        <<label::binary-size(label_len), data::bitstring>> = data
        labels([label | labels], data)
    end
  end

  @spec qr(0 | 1) :: :request | :response
  def qr(n) do
    case n do
      0 -> :request
      1 -> :response
      _ -> throw("Invalid QR value #{n}")
    end
  end

  @spec opcode(0 | 1 | 2) :: :iquery | :query | :status
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
