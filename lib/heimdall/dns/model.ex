defmodule Heimdall.DNS.Model do
  @type qr_atoms() :: :request | :response
  @type qr_ints() :: 0 | 1

  @type opcode_atoms() :: :query | :iquery | :status
  @type opcode_ints() :: 0 | 1 | 2

  @type rcode_atoms() :: :no_err | :server_err | :format_err | :name_err | :not_implemented | :refused
  @type rcode_ints() :: 0 | 1 | 2 | 3 | 4 | 5

  @type qtype_atoms() ::
          :a
          | :ns
          | :cname
          | :soa
          | :ptr
          | :hinfo
          | :mx
          | :txt
          | :rp
          | :afsdb
          | :sig
          | :key
          | :aaaa
          | :loc
          | :srv
          | :naptr
          | :kx
          | :cert
          | :dname
          | :opt
          | :apl
          | :ds
          | :sshfp
          | :ipseckey
          | :rrsig
          | :nsec
          | :dnskey
          | :dhcid
          | :nsec3
          | :nsec3param
          | :tlsa
          | :smimea
          | :hip
          | :cds
          | :cdnskey
          | :openpgpkey
          | :csync
          | :zonemd
          | :svcb
          | :https
          | :eui48
          | :eui64
          | :tkey
          | :tsig
          | :axfr
          | :mailb
          | :maila
          | :all
          | :uri
          | :caa
          | :ta
          | :dlv
  @type qtype_ints() ::
          1
          | 2
          | 5
          | 6
          | 12
          | 13
          | 15
          | 16
          | 17
          | 18
          | 24
          | 25
          | 28
          | 29
          | 33
          | 35
          | 36
          | 37
          | 39
          | 41
          | 42
          | 43
          | 44
          | 45
          | 46
          | 47
          | 48
          | 49
          | 50
          | 51
          | 52
          | 53
          | 55
          | 59
          | 60
          | 61
          | 62
          | 63
          | 64
          | 65
          | 108
          | 109
          | 249
          | 250
          | 252
          | 253
          | 254
          | 255
          | 256
          | 257
          | 32_768
          | 32_769

@type qclass_atoms() :: :in | :cs | :ch | :hesiod
@type qclass_ints() :: 1 | 2 | 3 | 4
end
