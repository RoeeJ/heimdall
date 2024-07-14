defmodule Heimdall.Schema.Record do
  alias Heimdall.Schema.Zone

  @type t() :: %__MODULE__{
          id: non_neg_integer(),
          name: String.t(),
          type: Heimdall.DNS.Model.qtype_atoms(),
          ttl: non_neg_integer(),
          data: map(),
          inserted_at: DateTime.t(),
          updated_at: DateTime.t(),
          zone_id: non_neg_integer(),
          zone: Zone.t()
        }
  use Ecto.Schema
  import Ecto.Changeset

  schema "dns_records" do
    field :name, :string

    field :type, Ecto.Enum,
      values: [
        :a,
        :ns,
        :cname,
        :soa,
        :ptr,
        :hinfo,
        :mx,
        :txt,
        :rp,
        :afsdb,
        :sig,
        :key,
        :aaaa,
        :loc,
        :srv,
        :naptr,
        :kx,
        :cert,
        :dname,
        :opt,
        :apl,
        :ds,
        :sshfp,
        :ipseckey,
        :rrsig,
        :nsec,
        :dnskey,
        :dhcid,
        :nsec3,
        :nsec3param,
        :tlsa,
        :smimea,
        :hip,
        :cds,
        :cdnskey,
        :openpgpkey,
        :csync,
        :zonemd,
        :svcb,
        :https,
        :eui48,
        :eui64,
        :tkey,
        :tsig,
        :axfr,
        :mailb,
        :maila,
        :all,
        :uri,
        :caa,
        :ta,
        :dlv
      ]

    field :ttl, :integer
    field :data, :map
    belongs_to :zone, Heimdall.DNS.Zone

    timestamps()
  end

  def changeset(record, attrs) do
    record
    |> cast(attrs, [:name, :type, :ttl, :data, :zone_id])
    |> validate_required([:name, :type, :data, :zone_id])
    |> validate_name()
  end

  defp validate_name(changeset) do
    case get_field(changeset, :name) do
      "@" -> put_change(changeset, :name, "")
      "" -> changeset
      name when is_binary(name) -> changeset
      _ -> add_error(changeset, :name, "must be a string or @")
    end
  end
end
