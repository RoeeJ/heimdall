defmodule Heimdall.Schema.Zone do
  @moduledoc """
  Zone schema.
  """
  @derive {Jason.Encoder, except: [:__meta__, :records, :soa]}
  alias Heimdall.Repo
  alias Heimdall.Schema.{Record, SOA}

  @type t() :: %__MODULE__{
          id: non_neg_integer(),
          inserted_at: DateTime.t(),
          updated_at: DateTime.t(),
          name: String.t(),
          records: [Record.t()],
          serial: non_neg_integer(),
          soa: SOA.t()
        }
  use Ecto.Schema
  import Ecto.Changeset
  alias Heimdall.Schema.{Record, SOA}

  schema "zones" do
    field :name, :string
    field :serial, :integer, default: 0

    has_one :soa, SOA
    has_many :records, Record

    timestamps()
  end

  def changeset(zone, attrs) do
    zone
    |> cast(attrs, [:name, :serial])
    |> validate_required([:name, :serial])
    |> unique_constraint(:name)
  end

  def all(),
    do:
      Repo.all(__MODULE__)
      |> Repo.preload(:records)
end
