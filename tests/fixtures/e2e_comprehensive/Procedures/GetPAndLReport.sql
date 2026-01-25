-- Procedure with ampersand in name (tests XML entity encoding)
CREATE PROCEDURE [dbo].[GetP&LReport]
    @StartDate DATE,
    @EndDate DATE,
    @IncludeDetails BIT = 0
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        @StartDate AS StartDate,
        @EndDate AS EndDate,
        @IncludeDetails AS IncludeDetails,
        'P&L Summary' AS ReportType;
END
GO
