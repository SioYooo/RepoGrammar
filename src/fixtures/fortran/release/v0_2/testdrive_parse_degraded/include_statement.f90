! Bounded parse-degraded fixture: an INCLUDE statement pulls text from
! another file, so the visible source is incomplete and the file abstains.
module include_stmt
  use testdrive
  implicit none

contains

  subroutine test_included(error)
    include 'assert_helpers.f90'
    type(error_type), allocatable, intent(out) :: error
  end subroutine test_included

end module include_stmt
