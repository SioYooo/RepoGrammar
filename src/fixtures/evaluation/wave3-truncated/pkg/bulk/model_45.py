from pydantic import BaseModel


class Item45(BaseModel):
    id: int
    label: str
    quantity: int
