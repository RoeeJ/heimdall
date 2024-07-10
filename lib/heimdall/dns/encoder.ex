defmodule Heimdall.DNS.Encoder do
  alias Heimdall.DNS.Model

  @spec packet(packet :: Model.Packet.t()) :: bitstring()
  def packet(%Model.Packet{} = packet) do
    data =
      <<packet.id::16, qr(packet.qr)::1, opcode(packet.opcode)::4, packet.aa::1, packet.tc::1,
        packet.rd::1, packet.ra::1, packet.z::3, packet.rcode::4, packet.qdcount::16,
        packet.ancount::16, packet.nscount::16, packet.arcount::16>>

    data =
      Enum.reduce(packet.questions, data, fn v, acc ->
        acc <> Model.Question.encode(v)
      end)

    data =
      Enum.reduce(packet.answers, data, fn v, acc ->
        acc <> Model.Answer.encode(v)
      end)

    data =
      Enum.reduce(packet.nameservers, data, fn v, acc ->
        acc <> Model.Nameserver.encode(v)
      end)

    data =
      Enum.reduce(packet.additional, data, fn v, acc ->
        acc <> Model.Additional.encode(v)
      end)

    data
  end

  def labels(label) do
    String.split(label, ".")
    |> Enum.reverse()
    |> Enum.reduce(<<0>>, fn v, acc ->
      <<String.length(v)::8, v::bitstring, acc::bitstring>>
    end)
  end

  @spec qr(:request | :response) :: 0 | 1
  def qr(n) do
    case n do
      :request -> 0
      :response -> 1
      _ -> throw("Invalid QR value #{n}")
    end
  end

  @spec opcode(:query | :iquery | :status) :: 0 | 1 | 2
  def opcode(n) do
    case n do
      :query -> 0
      :iquery -> 1
      :status -> 2
      _ -> throw("Invalid opcode value #{n}")
    end
  end

  def qtype(n) do
    case n do
      :a -> 1
      :ns -> 2
      :cname -> 5
      :soa -> 6
      :ptr -> 12
      :hinfo -> 13
      :mx -> 15
      :text -> 16
      :rp -> 17
      :afsdb -> 18
      :sig -> 24
      :key -> 25
      :aaaa -> 28
      :loc -> 29
      :srv -> 33
      :naptr -> 35
      :kx -> 36
      :cert -> 37
      :dname -> 39
      :opt -> 41
      :apl -> 42
      :ds -> 43
      :sshfp -> 44
      :ipseckey -> 45
      :rrsig -> 46
      :nsec -> 47
      :dnskey -> 48
      :dhcid -> 49
      :nsec3 -> 50
      :nsec3param -> 51
      :tlsa -> 52
      :smimea -> 53
      :hip -> 55
      :cds -> 59
      :cdnskey -> 60
      :openpgpkey -> 61
      :csync -> 62
      :zonemd -> 63
      :svcb -> 64
      :https -> 65
      :eui48 -> 108
      :eui64 -> 109
      :tkey -> 249
      :tsig -> 250
      :axfr -> 252
      :mailb -> 253
      :maila -> 254
      :all -> 255
      :uri -> 256
      :caa -> 257
      :ta -> 32768
      :dlv -> 32769
      _ -> throw("Unknown QTYPE value #{n}")
    end
  end

  def qclass(n) do
    case n do
      :in -> 1
      :cs -> 2
      :ch -> 3
      :hesiod -> 4
      _ -> throw("Unknown QCLASS value #{n}")
    end
  end
end
