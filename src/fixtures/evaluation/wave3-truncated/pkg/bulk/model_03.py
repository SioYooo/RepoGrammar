from pydantic import BaseModel


class Item03(BaseModel):
    id: int
    label: str
    quantity: int
