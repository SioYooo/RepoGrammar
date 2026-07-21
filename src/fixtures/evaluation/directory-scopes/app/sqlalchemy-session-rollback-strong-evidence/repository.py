from sqlalchemy.orm import Session


class UserRepository:
    def create_user(self, session: Session):
        session.rollback()

    def update_user(self, session: Session):
        session.rollback()

    def delete_user(self, session: Session):
        session.rollback()
